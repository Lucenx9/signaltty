//! Bounded explicit selected-file reads. Git owns tracked comparison and
//! normalization; untracked content is read through no-follow descriptors.

use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::process::Stdio;
use std::time::Duration;

use nix::libc;
use signaltty_core::diff::{parse_patch, untracked_text, DiffContent, FileDiff, MAX_PREVIEW_BYTES};
use signaltty_proto::code;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::time::timeout;

use crate::params::{bad_params, ParamError};

const READ_TIMEOUT: Duration = Duration::from_secs(8);
const STDERR_LIMIT: usize = 8 * 1024;

pub async fn read(cwd: &str, path: &str) -> Result<FileDiff, ParamError> {
    read_with_base(cwd, path, None).await
}

pub async fn read_with_base(
    cwd: &str,
    path: &str,
    base_sha: Option<&str>,
) -> Result<FileDiff, ParamError> {
    validate_path(path)?;
    timeout(READ_TIMEOUT, read_inner(cwd, path, base_sha))
        .await
        .map_err(|_| (code::TIMEOUT.into(), "File diff read timed out".into()))?
}

fn validate_path(path: &str) -> Result<(), ParamError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\0')
        || path
            .split('/')
            .any(|p| matches!(p, "" | "." | ".." | ".git"))
    {
        return Err(bad_params(
            "Expected one literal repository-relative file path",
        ));
    }
    Ok(())
}

fn unavailable(reason: impl Into<String>) -> DiffContent {
    DiffContent::Unavailable {
        reason: reason.into(),
    }
}

async fn read_inner(cwd: &str, path: &str, base_sha: Option<&str>) -> Result<FileDiff, ParamError> {
    let root = git(cwd, &["rev-parse", "--show-toplevel"], false).await?;
    if !root.success {
        return Err(bad_params("Workspace is not a Git checkout"));
    }
    let root = std::str::from_utf8(&root.bytes)
        .map_err(|_| bad_params("Git checkout path is not valid UTF-8"))?
        .strip_suffix('\n')
        .ok_or_else(|| bad_params("Invalid Git checkout root"))?
        .to_owned();
    let (base_exists, base_oid) = if let Some(sha) = base_sha {
        let verify = git(&root, &["rev-parse", "--verify", sha], false).await?;
        if !verify.success {
            return Err((code::IO_ERROR.into(), format!("missing base commit: {sha}")));
        }
        (true, sha.trim().to_string())
    } else {
        let head = git(&root, &["rev-parse", "--verify", "HEAD"], false).await?;
        if head.success {
            let oid = std::str::from_utf8(&head.bytes)
                .map_err(|_| bad_params("Invalid HEAD"))?
                .trim()
                .to_string();
            (true, oid)
        } else {
            (false, String::new())
        }
    };
    let tree = if base_exists {
        git(&root, &["ls-tree", "-z", &base_oid, "--", path], true)
            .await?
            .bytes
    } else {
        Vec::new()
    };
    let base_entry = tree.split(|b| *b == 0).find_map(|entry| {
        let tab = entry.iter().position(|b| *b == b'\t')?;
        (entry[tab + 1..] == *path.as_bytes()).then_some(&entry[..tab])
    });
    if base_entry.is_some_and(|entry| entry.starts_with(b"040000 ")) {
        return Err(bad_params("Expected one file, not a directory"));
    }
    let indexed = git(&root, &["ls-files", "--cached", "-z", "--", path], true).await?;
    let tracked = base_entry.is_some()
        || indexed
            .bytes
            .split(|b| *b == 0)
            .any(|p| p == path.as_bytes());
    let untracked = if tracked {
        false
    } else {
        let output = git(
            &root,
            &[
                "ls-files",
                "--others",
                "--exclude-standard",
                "-z",
                "--",
                path,
            ],
            true,
        )
        .await?;
        output
            .bytes
            .split(|b| *b == 0)
            .any(|p| p == path.as_bytes())
    };
    if !tracked && !untracked {
        return Err(bad_params(
            "File is not tracked or unignored untracked content",
        ));
    }
    let content = if tracked {
        let base = if base_exists {
            base_oid
        } else {
            let empty = git(&root, &["hash-object", "-t", "tree", "--stdin"], true).await?;
            String::from_utf8(empty.bytes)
                .map_err(|_| bad_params("Invalid empty tree"))?
                .trim()
                .to_owned()
        };
        let output = git(
            &root,
            &[
                "diff",
                "--no-color",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--no-relative",
                "--submodule=short",
                "--unified=3",
                &base,
                "--",
                path,
            ],
            true,
        )
        .await?;
        if output.too_large {
            unavailable("Preview exceeds the 512 KiB size limit.")
        } else if output.bytes.is_empty() {
            DiffContent::Unchanged
        } else {
            parse_patch(&output.bytes, false).unwrap_or_else(unavailable)
        }
    } else {
        let root_for_read = root.clone();
        let path_for_read = path.to_owned();
        let snapshot =
            tokio::task::spawn_blocking(move || snapshot(&root_for_read, &path_for_read))
                .await
                .map_err(|e| (code::IO_ERROR.into(), e.to_string()))?;
        match snapshot {
            Ok(Some(bytes)) if bytes.contains(&0) => DiffContent::Binary,
            Ok(Some(bytes)) => match std::str::from_utf8(&bytes) {
                Ok(text) => untracked_text(text),
                Err(_) => unavailable("Text is not valid UTF-8."),
            },
            Ok(None) => return Err(bad_params("File is no longer available")),
            Err(SnapshotError::Unavailable(reason)) => unavailable(reason),
            Err(SnapshotError::Io(error)) => {
                return Err((code::IO_ERROR.into(), error.to_string()))
            }
        }
    };
    Ok(FileDiff {
        path: path.into(),
        untracked,
        content,
    })
}

enum SnapshotError {
    Io(std::io::Error),
    Unavailable(&'static str),
}

fn snapshot(root: &str, path: &str) -> Result<Option<Vec<u8>>, SnapshotError> {
    let mut directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(root)
        .map_err(SnapshotError::Io)?;
    let components: Vec<_> = path.split('/').collect();
    for (index, component) in components.iter().enumerate() {
        let final_component = index == components.len() - 1;
        let component = CString::new(*component).expect("validated path contains no NUL");
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if final_component {
                0
            } else {
                libc::O_DIRECTORY
            };
        // Each open is relative to an owned directory descriptor, with
        // O_NOFOLLOW. Validated components cannot escape this directory.
        let fd = unsafe { libc::openat(directory.as_raw_fd(), component.as_ptr(), flags) };
        if fd < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(match error.raw_os_error() {
                Some(libc::ELOOP | libc::ENOTDIR) => {
                    SnapshotError::Unavailable("Preview unavailable for a symlink or unsafe path.")
                }
                Some(libc::ENXIO) => {
                    SnapshotError::Unavailable("Preview unavailable for a special file.")
                }
                _ => SnapshotError::Io(error),
            });
        }
        // A successful openat returns a new owned descriptor, consumed once.
        let file = unsafe { File::from_raw_fd(fd) };
        if !final_component {
            directory = file;
            continue;
        }
        let metadata = file.metadata().map_err(SnapshotError::Io)?;
        if !metadata.is_file() {
            return Err(SnapshotError::Unavailable(
                "Preview unavailable for a directory or special file.",
            ));
        }
        if metadata.len() > MAX_PREVIEW_BYTES as u64 {
            return Err(SnapshotError::Unavailable(
                "Preview exceeds the 512 KiB size limit.",
            ));
        }
        let mut bytes = Vec::new();
        file.take((MAX_PREVIEW_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(SnapshotError::Io)?;
        if bytes.len() > MAX_PREVIEW_BYTES {
            return Err(SnapshotError::Unavailable(
                "Preview exceeds the 512 KiB size limit.",
            ));
        }
        return Ok(Some(bytes));
    }
    unreachable!("validated nonempty path")
}

pub(crate) struct ProcessGroup(pub(crate) Option<i32>);

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
    }
}

struct GitOutput {
    bytes: Vec<u8>,
    success: bool,
    too_large: bool,
}

async fn bounded(reader: impl AsyncRead + Unpin, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > limit {
        return Err(if limit == MAX_PREVIEW_BYTES {
            std::io::Error::new(
                std::io::ErrorKind::FileTooLarge,
                "Git output exceeds preview limit",
            )
        } else {
            std::io::Error::other("Git stderr exceeds the 8 KiB diagnostic limit")
        });
    }
    Ok(bytes)
}

async fn git(cwd: &str, args: &[&str], require_success: bool) -> Result<GitOutput, ParamError> {
    let mut child = tokio::process::Command::new("git")
        .arg("--literal-pathspecs")
        .arg("-C")
        .arg(cwd)
        .args([
            "-c",
            "diff.relative=false",
            "-c",
            "diff.external=",
            "-c",
            "core.pager=cat",
            "-c",
            "diff.suppressBlankEmpty=false",
            "-c",
            "diff.outputIndicatorNew=+",
            "-c",
            "diff.outputIndicatorOld=-",
            "-c",
            "diff.outputIndicatorContext= ",
        ])
        .args(args)
        .env_remove("GIT_DIFF_OPTS")
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| (code::IO_ERROR.into(), e.to_string()))?;
    let mut group = ProcessGroup(Some(child.id().unwrap() as i32));
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let output = tokio::try_join!(
        child.wait(),
        bounded(stdout, MAX_PREVIEW_BYTES),
        bounded(stderr, STDERR_LIMIT)
    );
    match output {
        Ok((status, bytes, error)) => {
            // The child is reaped; avoid killing a recycled process group id.
            group.0 = None;
            if require_success && !status.success() {
                return Err((
                    code::IO_ERROR.into(),
                    String::from_utf8_lossy(&error).trim().into(),
                ));
            }
            Ok(GitOutput {
                bytes,
                success: status.success(),
                too_large: false,
            })
        }
        Err(error) => {
            drop(group);
            let _ = child.wait().await;
            if error.kind() == std::io::ErrorKind::FileTooLarge {
                Ok(GitOutput {
                    bytes: Vec::new(),
                    success: false,
                    too_large: true,
                })
            } else {
                Err((code::IO_ERROR.into(), error.to_string()))
            }
        }
    }
}
