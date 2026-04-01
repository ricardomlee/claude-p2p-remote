//! P2P Claude Code Signaling Server - Main entry point

use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting P2P Claude Code Signaling Server...");

    // TODO: Initialize Axum server
    // TODO: Setup WebSocket endpoint
    // TODO: Start server

    tracing::info!("Signaling server started successfully");

    Ok(())
}
