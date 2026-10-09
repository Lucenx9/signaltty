//! Nonblocking native PTY descriptors: bounded input, blocking output reads.

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use signaltty_proto::code;

pub const INPUT_TIMEOUT: Duration = Duration::from_secs(5);
const CANCEL_TICK_MS: u16 = 20;

#[derive(Debug)]
pub struct InputError {
    pub code: &'static str,
    pub message: String,
    pub written_bytes: Option<usize>,
}

impl InputError {
    pub fn new(code: &'static str, message: impl Into<String>, written: usize) -> Self {
        Self {
            code,
            message: message.into(),
            written_bytes: Some(written),
        }
    }

    pub fn check(canceled: &AtomicBool, deadline: Instant, written: usize) -> Result<(), Self> {
        if canceled.load(Ordering::Acquire) {
            Err(Self::new(code::PANE_EXITED, "PTY input canceled", written))
        } else if Instant::now() >= deadline {
            Err(Self::new(code::TIMEOUT, "PTY input timed out", written))
        } else {
            Ok(())
        }
    }
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.written_bytes {
            Some(n) => write!(f, "{} ({n} bytes accepted)", self.message),
            None => write!(f, "{} (accepted byte count unavailable)", self.message),
        }
    }
}

pub fn open(master: &dyn portable_pty::MasterPty) -> io::Result<(File, PtyReader)> {
    let raw = master
        .as_raw_fd()
        .ok_or_else(|| io::Error::other("native PTY has no descriptor"))?;
    let flags = OFlag::from_bits_truncate(fcntl(raw, FcntlArg::F_GETFL)?);
    fcntl(raw, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
    // The master remains owned by the caller while its descriptor is borrowed
    // and duplicated. Both clones share O_NONBLOCK; the reader accounts for it.
    let fd = unsafe { BorrowedFd::borrow_raw(raw) };
    Ok((
        File::from(fd.try_clone_to_owned()?),
        PtyReader(File::from(fd.try_clone_to_owned()?)),
    ))
}

pub fn write(
    file: &mut File,
    data: &[u8],
    deadline: Instant,
    canceled: &AtomicBool,
) -> Result<usize, InputError> {
    let mut written = 0;
    while written < data.len() {
        InputError::check(canceled, deadline, written)?;
        match file.write(&data[written..]) {
            Ok(0) => {
                return Err(InputError::new(
                    code::IO_ERROR,
                    "PTY accepted zero input bytes",
                    written,
                ))
            }
            Ok(n) => written += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                let mut fds = [PollFd::new(file.as_fd(), PollFlags::POLLOUT)];
                let remaining = deadline.saturating_duration_since(Instant::now());
                let wait_ms = remaining.as_millis().min(u128::from(CANCEL_TICK_MS)) as u16;
                match poll(&mut fds, wait_ms) {
                    Ok(_) => {
                        if fds[0].revents().unwrap_or(PollFlags::empty()).intersects(
                            PollFlags::POLLHUP | PollFlags::POLLERR | PollFlags::POLLNVAL,
                        ) {
                            return Err(InputError::new(
                                code::PANE_EXITED,
                                "PTY input closed",
                                written,
                            ));
                        }
                    }
                    Err(nix::errno::Errno::EINTR) => continue,
                    Err(e) => return Err(InputError::new(code::IO_ERROR, e.to_string(), written)),
                }
            }
            Err(e) => {
                return Err(InputError::new(
                    if e.raw_os_error() == Some(nix::libc::EIO) {
                        code::PANE_EXITED
                    } else {
                        code::IO_ERROR
                    },
                    e.to_string(),
                    written,
                ))
            }
        }
    }
    Ok(written)
}

pub struct PtyReader(File);

impl Read for PtyReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.0.read(buf) {
                Err(e) if e.raw_os_error() == Some(nix::libc::EIO) => return Ok(0),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    let mut fds = [PollFd::new(self.0.as_fd(), PollFlags::POLLIN)];
                    match poll(&mut fds, PollTimeout::NONE) {
                        Ok(_) => {
                            let flags = fds[0].revents().unwrap_or(PollFlags::empty());
                            if !flags.contains(PollFlags::POLLIN)
                                && flags.intersects(
                                    PollFlags::POLLHUP | PollFlags::POLLERR | PollFlags::POLLNVAL,
                                )
                            {
                                return Ok(0);
                            }
                        }
                        Err(nix::errno::Errno::EINTR) => continue,
                        Err(e) => return Err(e.into()),
                    }
                }
                result => return result,
            }
        }
    }
}
