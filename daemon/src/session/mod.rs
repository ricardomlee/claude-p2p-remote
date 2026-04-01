//! Session management layer

pub mod manager;
pub mod claude;

pub use claude::{ClaudeOutput, ClaudePty, ConfirmMode};
pub use manager::SessionManager;
