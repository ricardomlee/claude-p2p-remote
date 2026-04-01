//! Claude CLI PTY wrapper
//!
//! Spawns and interacts with Claude Code CLI via pseudo-terminal

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize, PtyPair};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Claude PTY session
pub struct ClaudePty {
    /// PTY writer (stdin)
    writer: Arc<Mutex<Box<dyn portable_pty::Writer + Send + Sync>>>,
    /// PTY reader handle
    _reader: Box<dyn portable_pty::Reader + Send>,
    /// Child process handle
    child: Box<dyn portable_pty::Child + Send + Sync>,
    /// Confirmation mode
    confirm_mode: Arc<Mutex<ConfirmMode>>,
}

/// Confirmation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmMode {
    /// Auto-approve all confirmations
    Auto,
    /// Forward confirmations to client
    Manual,
}

/// Output event from Claude
#[derive(Debug, Clone)]
pub enum ClaudeOutput {
    /// Regular output chunk
    Output { text: String },
    /// Claude finished response
    Done,
    /// Confirmation required
    NeedsConfirmation { prompt: String },
    /// Error occurred
    Error { message: String },
}

impl ClaudePty {
    /// Spawn a new Claude CLI process
    pub fn spawn(confirm_mode: ConfirmMode) -> Result<Self> {
        // Get PTY system
        let pty_system = native_pty_system();

        // Create a new pseudo-terminal
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to open PTY")?;

        // Build command for Claude CLI
        let mut cmd = CommandBuilder::new("claude");
        cmd.args(&["--cli"]); // Run in CLI mode

        // Spawn the process
        let child = pair
            .slave
            .spawn_command(cmd)
            .context("Failed to spawn Claude CLI")?;

        let writer = pair.master.take_writer().context("Failed to get writer")?;
        let reader = pair.master.try_clone_reader().context("Failed to get reader")?;

        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            _reader: Box::new(reader),
            child,
            confirm_mode: Arc::new(Mutex::new(confirm_mode)),
        })
    }

    /// Set confirmation mode
    pub fn set_confirm_mode(&self, mode: ConfirmMode) {
        if let Ok(mut current) = self.confirm_mode.lock() {
            *current = mode;
        }
    }

    /// Get current confirmation mode
    pub fn get_confirm_mode(&self) -> ConfirmMode {
        self.confirm_mode
            .lock()
            .map(|m| *m)
            .unwrap_or(ConfirmMode::Auto)
    }

    /// Send a message to Claude
    pub fn send_message(&self, message: &str) -> Result<()> {
        let writer = self.writer.lock().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        writeln!(writer, "{}", message).context("Failed to write to Claude")?;
        Ok(())
    }

    /// Send acknowledgment (for manual confirm mode)
    pub fn send_ack(&self) -> Result<()> {
        self.send_message("y")
    }

    /// Send denial (for manual confirm mode)
    pub fn send_deny(&self) -> Result<()> {
        self.send_message("n")
    }

    /// Kill the Claude process
    pub fn kill(&mut self) -> Result<()> {
        self.child.kill().context("Failed to kill Claude process")
    }

    /// Check if process is still running
    pub fn is_running(&self) -> bool {
        self.child.try_wait().map(|o| o.is_none()).unwrap_or(false)
    }
}

/// Stream reader for Claude output
pub struct ClaudeOutputStream {
    receiver: mpsc::Receiver<ClaudeOutput>,
}

impl ClaudeOutputStream {
    /// Create a new output stream
    pub fn new(pty: Arc<ClaudePty>) -> Self {
        let (tx, rx) = mpsc::channel(100);

        // Spawn thread to read from PTY
        std::thread::spawn(move || {
            // TODO: Implement actual PTY reading
            // This is a placeholder
            let _ = pty;
            let _ = tx;
        });

        Self { receiver: rx }
    }

    /// Receive next output event
    pub async fn recv(&mut self) -> Option<ClaudeOutput> {
        self.receiver.recv().await
    }
}
