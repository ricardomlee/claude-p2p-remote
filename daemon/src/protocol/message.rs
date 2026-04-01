//! Protocol message types for P2P Claude Code communication

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Client -> Server messages
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Chat message
    Chat {
        message: String,
        conversation_id: Option<Uuid>,
    },
    /// Chat with media attachment
    ChatWithMedia {
        message: String,
        media: MediaRef,
    },
    /// List directory contents
    FileList { path: String },
    /// Read file content
    FileRead { path: String },
    /// Write file content
    FileWrite { path: String, content: String },
    /// Switch confirmation mode
    SetConfirmMode { mode: ConfirmMode },
    /// Acknowledge/confirm action
    Ack,
}

/// Server -> Client messages
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Streaming chat response chunk
    ChatChunk { text: String },
    /// Chat response completed
    ChatDone { conversation_id: Uuid },
    /// Directory listing result
    FileList { entries: Vec<FileEntry> },
    /// File content
    FileContent { content: String },
    /// File write acknowledgment
    FileWritten { path: String },
    /// Command output
    CommandOutput {
        stdout: String,
        stderr: String,
        exit_code: i32,
    },
    /// Error message
    Error { code: String, message: String },
    /// Acknowledgment required from user
    NeedAck { prompt: String },
}

/// Media reference for multimedia messages
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MediaRef {
    pub id: String,
    #[serde(rename = "type")]
    pub ty: MediaType,
}

/// Media type enumeration
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    /// Audio (voice, can be transcribed)
    Audio,
    /// Image
    Image,
    /// Video
    Video,
}

/// Confirmation mode for Claude CLI interactions
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmMode {
    /// Auto-approve all confirmations
    Auto,
    /// Forward confirmations to client for approval
    Manual,
}

/// File entry for directory listings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: FileType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
}

/// File type enumeration
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum FileType {
    File = 0,
    Dir = 1,
    Symlink = 2,
}
