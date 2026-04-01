//! Session manager - multiplexes multiple clients to single Claude process

use super::claude::{ClaudeOutput, ClaudePty, ConfirmMode};
use crate::protocol::{ClientMessage, ServerMessage};
use anyhow::Result;
use dashmap::DashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

/// Client identifier
pub type ClientId = String;

/// Client sender for broadcasting messages
type ClientSender = broadcast::Sender<ServerMessage>;

/// Session manager state
pub struct SessionManager {
    /// Connected clients
    clients: DashMap<ClientId, ClientSender>,
    /// Shared Claude PTY instance
    claude: Arc<ClaudePty>,
    /// Current pending acknowledgment (client waiting for confirmation)
    pending_ack: Arc<Mutex<Option<ClientId>>>,
    /// Current conversation ID
    conversation_id: Arc<Mutex<Option<Uuid>>>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(confirm_mode: ConfirmMode) -> Result<Self> {
        let claude = ClaudePty::spawn(confirm_mode)?;
        let claude = Arc::new(claude);

        // TODO: Start output reader task
        // let _output_stream = ClaudeOutputStream::new(claude.clone());

        Ok(Self {
            clients: DashMap::new(),
            claude,
            pending_ack: Arc::new(Mutex::new(None)),
            conversation_id: Arc::new(Mutex::new(None)),
        })
    }

    /// Add a new client connection
    pub fn add_client(&self, client_id: ClientId) -> broadcast::Receiver<ServerMessage> {
        let (tx, rx) = broadcast::channel(100);
        self.clients.insert(client_id, tx);
        rx
    }

    /// Remove a client
    pub fn remove_client(&self, client_id: &str) {
        self.clients.remove(client_id);
    }

    /// Get number of connected clients
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Route a client message
    pub async fn route_message(
        &self,
        client_id: &ClientId,
        msg: ClientMessage,
    ) -> Result<(), String> {
        match msg {
            ClientMessage::Chat {
                message,
                conversation_id,
            } => {
                // Check if another client is waiting for confirmation
                if self.is_pending_ack(client_id) {
                    return Err("Waiting for confirmation from another client".into());
                }

                // Update conversation ID if provided
                if let Some(cid) = conversation_id {
                    if let Ok(mut current) = self.conversation_id.lock() {
                        *current = Some(cid);
                    }
                }

                // Send message to Claude
                self.claude
                    .send_message(&message)
                    .map_err(|e| e.to_string())?;

                // Response will be broadcast via output stream
                Ok(())
            }

            ClientMessage::ChatWithMedia { message, media } => {
                if self.is_pending_ack(client_id) {
                    return Err("Waiting for confirmation from another client".into());
                }

                // Format message with media reference
                let formatted = format!(
                    "[Media: {:?}] {}",
                    media.ty, message
                );
                self.claude
                    .send_message(&formatted)
                    .map_err(|e| e.to_string())?;
                Ok(())
            }

            ClientMessage::SetConfirmMode { mode } => {
                self.claude.set_confirm_mode(mode);
                // Broadcast mode change to all clients
                self.broadcast(&ServerMessage::ChatDone {
                    conversation_id: Uuid::new_v4(), // Placeholder
                });
                Ok(())
            }

            ClientMessage::Ack => {
                // Clear pending acknowledgment
                if let Ok(mut pending) = self.pending_ack.lock() {
                    if pending.as_ref() == Some(&client_id.to_string()) {
                        *pending = None;
                        self.claude.send_ack().map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            }

            ClientMessage::FileList { path } => {
                // TODO: Implement file listing
                let _ = path;
                Ok(())
            }

            ClientMessage::FileRead { path } => {
                // TODO: Implement file reading
                let _ = path;
                Ok(())
            }

            ClientMessage::FileWrite { path, content } => {
                // TODO: Implement file writing
                let _ = path;
                let _ = content;
                Ok(())
            }
        }
    }

    /// Check if this client is waiting for acknowledgment
    fn is_pending_ack(&self, client_id: &str) -> bool {
        self.pending_ack
            .lock()
            .map(|p| p.as_ref().map(|c| c != client_id).unwrap_or(false))
            .unwrap_or(false)
    }

    /// Broadcast a message to all clients
    fn broadcast(&self, msg: &ServerMessage) {
        for entry in self.clients.iter() {
            let _ = entry.value().send(msg.clone());
        }
    }

    /// Handle Claude output event
    pub fn handle_output(&self, output: ClaudeOutput) {
        match output {
            ClaudeOutput::Output { text } => {
                self.broadcast(&ServerMessage::ChatChunk { text });
            }
            ClaudeOutput::Done => {
                let conv_id = self
                    .conversation_id
                    .lock()
                    .ok()
                    .and_then(|c| *c)
                    .unwrap_or_else(Uuid::new_v4);
                self.broadcast(&ServerMessage::ChatDone {
                    conversation_id: conv_id,
                });
            }
            ClaudeOutput::NeedsConfirmation { prompt } => {
                // Check current mode
                match self.claude.get_confirm_mode() {
                    ConfirmMode::Auto => {
                        // Auto-approve
                        let _ = self.claude.send_ack();
                    }
                    ConfirmMode::Manual => {
                        // Set pending and notify all clients
                        if let Ok(mut pending) = self.pending_ack.lock() {
                            *pending = None; // Any client can confirm
                        }
                        self.broadcast(&ServerMessage::NeedAck { prompt });
                    }
                }
            }
            ClaudeOutput::Error { message } => {
                self.broadcast(&ServerMessage::Error {
                    code: "claude_error".into(),
                    message,
                });
            }
        }
    }
}
