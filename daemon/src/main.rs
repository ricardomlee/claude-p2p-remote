//! P2P Claude Code Daemon - Main entry point

use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod protocol;
mod webrtc;
mod session;
mod fs;
mod config;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting P2P Claude Code Daemon...");

    // TODO: Load configuration
    // TODO: Initialize WebRTC connection
    // TODO: Connect to signaling server
    // TODO: Wait for pairing
    // TODO: Start main event loop

    tracing::info!("Daemon started successfully");

    Ok(())
}
