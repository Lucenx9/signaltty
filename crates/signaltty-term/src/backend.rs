//! Backend trait so GUI code never depends on a concrete engine.
//! Implementations: HeadlessBackend (server, now), VTE (GUI, Phase 3),
//! GhosttyVt (future, feature-gated).

#[derive(Debug, Clone, Default)]
pub struct TermOptions {
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub audible_bell: bool,
}

pub trait TerminalBackend {
    fn create_surface(&mut self, id: &str, cols: u16, rows: u16);
    fn feed_output(&mut self, id: &str, data: &[u8]);
    /// Client-side input path (forwards to server IPC in real clients).
    fn send_input(&mut self, id: &str, data: &[u8]);
    fn resize(&mut self, id: &str, cols: u16, rows: u16);
    /// Plain-text screen snapshot (for reading, never for replay).
    fn snapshot(&self, id: &str) -> String;
    /// Replayable screen state — VT bytes that redraw contents,
    /// attributes, cursor and input modes on a fresh emulator. What
    /// attaching clients feed before the live stream.
    fn screen_state(&self, id: &str) -> Vec<u8>;
    fn configure(&mut self, opts: &TermOptions);
    fn destroy(&mut self, id: &str);
}
