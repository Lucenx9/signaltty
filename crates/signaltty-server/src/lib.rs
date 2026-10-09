//! signaltty session server: owns PTYs, processes, state, IPC.

pub mod approvals;
pub mod attrib;
pub mod audit;
mod codex;
pub mod config;
pub mod file_diff;
pub mod git;
mod logind;
pub mod params;
pub mod persist;
pub mod procscan;
pub mod pty;
pub mod pty_io;
pub mod router;
pub mod server;
pub mod store;
pub mod submit;
pub mod tasks;
pub mod worktrees;

pub use config::Config;
pub use server::serve;
