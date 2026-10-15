//! Owns command lifetime, including cancellation while descendants hold output pipes.

use std::io;
use std::process::{Output, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};

#[derive(Default)]
pub(crate) struct CaptureLimits {
    pub stdout: Option<usize>,
    pub stderr: Option<usize>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("{0}")]
    Spawn(io::Error),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("command timed out")]
    Timeout,
    #[error("stdout exceeds capture limit")]
    StdoutLimit,
    #[error("stderr exceeds capture limit")]
    StderrLimit,
}

struct OwnedChild(Child);

impl OwnedChild {
    fn kill_group(&self) {
        if let Some(pid) = self.0.id() {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        self.kill_group();
        // Child's kill_on_drop transfers eventual reaping to Tokio on cancellation.
    }
}

/// Runs a prepared command with null stdin and concurrent output capture.
/// Callers retain command policy and interpret exit status, limits and errors.
pub(crate) async fn run(
    command: &mut Command,
    deadline: Option<Duration>,
    limits: CaptureLimits,
) -> Result<Output, Error> {
    let mut child = OwnedChild(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .kill_on_drop(true)
            .spawn()
            .map_err(Error::Spawn)?,
    );
    let capture = async {
        let stdout = child.0.stdout.take().unwrap();
        let stderr = child.0.stderr.take().unwrap();
        let (stdout, stderr) = tokio::try_join!(
            read(stdout, limits.stdout, Error::StdoutLimit),
            read(stderr, limits.stderr, Error::StderrLimit),
        )?;
        // Keep the leader unreaped until pipes close, pinning its process-group ID
        // even if a descendant outlives it. No group signal follows a successful wait.
        let status = child.0.wait().await?;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    };
    let result = match deadline {
        Some(deadline) => tokio::time::timeout(deadline, capture)
            .await
            .unwrap_or(Err(Error::Timeout)),
        None => capture.await,
    };
    if result.is_err() {
        child.kill_group();
        let _ = child.0.wait().await;
    }
    result
}

async fn read(
    mut reader: impl AsyncRead + Unpin,
    limit: Option<usize>,
    exceeded: Error,
) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    if let Some(limit) = limit {
        reader
            .take((limit as u64).saturating_add(1))
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > limit {
            return Err(exceeded);
        }
    } else {
        reader.read_to_end(&mut bytes).await?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn shell(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(signaltty_core::ids::new_pane_id());
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn command(&self, leader_exits: bool) -> Command {
            let mut command = shell(if leader_exits {
                "printf '%s' $$ > leader; (printf ready > ready; sleep 1; printf late > late) & exit 0"
            } else {
                "printf '%s' $$ > leader; (printf ready > ready; sleep 1; printf late > late) & wait"
            });
            command.current_dir(&self.0);
            command
        }

        async fn ready(&self) {
            tokio::time::timeout(Duration::from_secs(3), async {
                while !self.0.join("ready").exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("descendant must start");
        }

        async fn leader_exited(&self) {
            let pid = std::fs::read_to_string(self.0.join("leader")).unwrap();
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
                        .expect("leader must remain owned and unreaped");
                    if stat
                        .rsplit_once(") ")
                        .is_some_and(|(_, fields)| fields.starts_with('Z'))
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("leader must exit before cancellation");
        }

        async fn stopped(&self) {
            tokio::time::sleep(Duration::from_millis(1200)).await;
            assert!(!self.0.join("late").exists(), "descendant kept running");
            let pid = std::fs::read_to_string(self.0.join("leader")).unwrap();
            assert!(
                !std::path::Path::new(&format!("/proc/{pid}")).exists(),
                "direct child was not reaped"
            );
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn captures_both_full_pipes_and_nonzero_status_with_stdin_closed() {
        let output = run(
            &mut shell("read -r input && exit 9; head -c 131072 /dev/zero >&2; head -c 131072 /dev/zero; exit 7"),
            Some(Duration::from_secs(3)),
            CaptureLimits::default(),
        )
        .await
        .unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(output.stdout, vec![0; 131072]);
        assert_eq!(output.stderr, vec![0; 131072]);
    }

    #[tokio::test]
    async fn preserves_spawn_failure() {
        let fixture = Fixture::new();
        let result = run(
            &mut Command::new(fixture.0.join("missing-executable")),
            None,
            CaptureLimits::default(),
        )
        .await;
        assert!(
            matches!(result, Err(Error::Spawn(error)) if error.kind() == io::ErrorKind::NotFound)
        );
    }

    #[tokio::test]
    async fn accepts_exact_limits_and_distinguishes_each_stream_overflow() {
        let limits = || CaptureLimits {
            stdout: Some(4),
            stderr: Some(4),
        };
        let output = run(&mut shell("printf 1234; printf abcd >&2"), None, limits())
            .await
            .unwrap();
        assert_eq!(output.stdout, b"1234");
        assert_eq!(output.stderr, b"abcd");
        assert!(matches!(
            run(&mut shell("printf 12345"), None, limits()).await,
            Err(Error::StdoutLimit)
        ));
        assert!(matches!(
            run(&mut shell("printf abcde >&2"), None, limits()).await,
            Err(Error::StderrLimit)
        ));
    }

    #[tokio::test]
    async fn deadline_stops_descendants_even_after_the_leader_exits() {
        for leader_exits in [false, true] {
            let fixture = Fixture::new();
            let result = run(
                &mut fixture.command(leader_exits),
                Some(Duration::from_millis(500)),
                CaptureLimits::default(),
            )
            .await;
            assert!(fixture.0.join("ready").exists());
            assert!(matches!(result, Err(Error::Timeout)));
            fixture.stopped().await;
        }
    }

    #[tokio::test]
    async fn cancellation_stops_descendants_even_after_the_leader_exits() {
        for leader_exits in [false, true] {
            let fixture = Fixture::new();
            let mut command = fixture.command(leader_exits);
            let task =
                tokio::spawn(
                    async move { run(&mut command, None, CaptureLimits::default()).await },
                );
            fixture.ready().await;
            if leader_exits {
                fixture.leader_exited().await;
            }
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
            fixture.stopped().await;
        }
    }

    #[tokio::test]
    async fn deadline_stops_a_process_that_closed_its_output_before_exiting() {
        let fixture = Fixture::new();
        let mut command = shell("printf '%s' $$ > leader; printf ready > ready; exec 1>&- 2>&-; sleep 1; printf late > late");
        command.current_dir(&fixture.0);
        let result = run(
            &mut command,
            Some(Duration::from_millis(500)),
            CaptureLimits::default(),
        )
        .await;
        assert!(matches!(result, Err(Error::Timeout)));
        fixture.stopped().await;
    }

    #[tokio::test]
    async fn overflow_stops_descendants_while_the_other_pipe_is_open() {
        let fixture = Fixture::new();
        // The overflow happens only after the descendant has published readiness.
        let mut command = shell("printf '%s' $$ > leader; (printf ready > ready; sleep 1; printf late > late) & while [ ! -f ready ]; do :; done; printf 12345; wait");
        command.current_dir(&fixture.0);
        let result = run(
            &mut command,
            Some(Duration::from_secs(3)),
            CaptureLimits {
                stdout: Some(4),
                stderr: None,
            },
        )
        .await;
        assert!(matches!(result, Err(Error::StdoutLimit)));
        fixture.stopped().await;
    }
}
