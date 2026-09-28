//! Terminal abstraction: `TerminalBackend` trait, headless `vt100`
//! implementation, OSC 9/99/777 notification scanner, sanitizers.
//! See docs/05.

pub mod backend;
pub mod headless;
pub mod osc;
pub mod sanitize;

pub use backend::{TermOptions, TerminalBackend};
pub use headless::HeadlessBackend;
pub use osc::{OscEvent, OscScanner};
pub use sanitize::{sanitize_notification_text, strip_ansi};
