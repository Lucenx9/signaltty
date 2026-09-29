//! signaltty session server: owns PTYs, processes, state, IPC.

pub mod attrib;
pub mod audit;
pub mod config;
pub mod git;
pub mod params;
pub mod persist;
pub mod procscan;
pub mod pty;
pub mod router;
pub mod server;
pub mod store;

pub use config::Config;
pub use server::serve;
