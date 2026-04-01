//! Authentication and pairing
//!
//! Handles client authentication and pairing codes

use anyhow::{Context, Result};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Pairing code for client authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingCode {
    /// The code string (6 digits)
    pub code: String,
    /// Expiration timestamp (Unix epoch seconds)
    pub expires_at: u64,
    /// Associated client ID
    pub client_id: Option<String>,
}

impl PairingCode {
    /// Generate a new pairing code
    pub fn new(expires_in_secs: u64) -> Self {
        let code = format!("{:06}", rand::rng().random_range(0..1_000_000));
        let expires_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + expires_in_secs;

        Self {
            code,
            expires_at,
            client_id: None,
        }
    }

    /// Check if the code has expired
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now > self.expires_at
    }

    /// Check if the code matches
    pub fn matches(&self, code: &str) -> bool {
        self.code == code && !self.is_expired()
    }
}

/// Authentication manager
pub struct AuthManager {
    /// Current pairing code
    current_code: Arc<RwLock<Option<PairingCode>>>,
    /// API key for Claude API
    api_key: String,
    /// Allowed client IDs (empty = allow all)
    allowed_clients: Arc<RwLock<Vec<String>>>,
}

impl AuthManager {
    /// Create a new auth manager
    pub fn new(api_key: String) -> Self {
        Self {
            current_code: Arc::new(RwLock::new(None)),
            api_key,
            allowed_clients: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Generate a new pairing code
    pub async fn generate_pairing_code(&self, expires_in_secs: u64) -> String {
        let code = PairingCode::new(expires_in_secs);
        let code_str = code.code.clone();

        let mut current = self.current_code.write().await;
        *current = Some(code);

        code_str
    }

    /// Validate a pairing code
    pub async fn validate_pairing_code(&self, code: &str) -> Result<()> {
        let current = self.current_code.read().await;

        match current.as_ref() {
            Some(pc) if pc.matches(code) => Ok(()),
            Some(_) => Err(anyhow::anyhow!("Invalid or expired pairing code")),
            None => Err(anyhow::anyhow!("No active pairing code")),
        }
    }

    /// Get the API key
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Add an allowed client
    pub async fn add_allowed_client(&self, client_id: String) {
        let mut clients = self.allowed_clients.write().await;
        clients.push(client_id);
    }

    /// Check if a client is allowed
    pub async fn is_client_allowed(&self, client_id: &str) -> bool {
        let clients = self.allowed_clients.read().await;
        clients.is_empty() || clients.contains(&client_id.to_string())
    }

    /// Clear the current pairing code
    pub async fn clear_pairing_code(&self) {
        let mut current = self.current_code.write().await;
        *current = None;
    }
}

/// Configuration for the daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// API key for Claude API
    pub api_key: String,
    /// Signaling server URL
    pub signaling_url: String,
    /// STUN servers for WebRTC
    pub stun_servers: Vec<String>,
    /// Root directory for file operations
    pub root_path: String,
    /// Default confirmation mode
    pub confirm_mode: String,
    /// Listening port for local mode
    pub listen_port: u16,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            api_key: std::env::var("ANTHROPIC_API_KEY")
                .unwrap_or_else(|_| String::new()),
            signaling_url: "ws://localhost:8080/ws".to_string(),
            stun_servers: vec!["stun.l.google.com:19302".to_string()],
            root_path: ".".to_string(),
            confirm_mode: "auto".to_string(),
            listen_port: 8081,
        }
    }
}

impl DaemonConfig {
    /// Load configuration from file or environment
    pub fn load() -> Result<Self> {
        // Try to load from config file
        let config_path = std::env::var("CLAUDE_P2P_CONFIG")
            .unwrap_or_else(|_| "config.json".to_string());

        if std::path::Path::new(&config_path).exists() {
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config: {}", config_path))?;
            let config: DaemonConfig = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse config: {}", config_path))?;
            return Ok(config);
        }

        // Fall back to defaults
        Ok(Self::default())
    }

    /// Save configuration to file
    pub fn save(&self, path: &str) -> Result<()> {
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize config")?;
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config: {}", path))?;
        Ok(())
    }
}
