//! P2P Claude Code Daemon - Main entry point
//!
//! Main entry point for the daemon that:
//! - Loads configuration
//! - Initializes WebRTC connection
//! - Connects to signaling server
//! - Manages Claude CLI session
//! - Handles client messages

use anyhow::{Context, Result};
use bytes::Bytes;
use clap::Parser;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod protocol;
mod webrtc;
mod session;
mod fs;
mod config;

use config::{AuthManager, DaemonConfig};
use protocol::{ClientMessage, ConfirmMode, ServerMessage};
use session::{ClaudePty, ConfirmMode as SessionConfirmMode, SessionManager};
use webrtc::{SignalingClient, WebRtcConnection};

/// P2P Claude Code Daemon
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "config.json")]
    config: String,

    /// Confirmation mode: auto or manual
    #[arg(long, default_value = "auto")]
    confirm_mode: String,

    /// Signaling server URL
    #[arg(long)]
    signaling: Option<String>,

    /// Run in host mode (wait for clients)
    #[arg(long)]
    host: bool,

    /// Connect as client with pairing code
    #[arg(long)]
    pair: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    // Load configuration
    let config = DaemonConfig::load().context("Failed to load configuration")?;

    tracing::info!("Starting P2P Claude Code Daemon...");
    tracing::info!("Config: signaling={}, confirm_mode={}", config.signaling_url, config.confirm_mode);

    // Create auth manager
    let auth = AuthManager::new(config.api_key.clone());

    // Determine confirm mode
    let confirm_mode = match args.confirm_mode.as_str() {
        "manual" => SessionConfirmMode::Manual,
        _ => SessionConfirmMode::Auto,
    };

    // Create session manager with Claude PTY
    let session_manager = SessionManager::new(confirm_mode)
        .context("Failed to create session manager")?;

    let session_manager = Arc::new(session_manager);

    if args.host {
        // Run in host mode - wait for clients to connect
        run_host_mode(config, auth, session_manager).await?;
    } else if let Some(pairing_code) = args.pair {
        // Run in client mode - connect to host
        run_client_mode(config, auth, session_manager, &pairing_code).await?;
    } else {
        // Default: show pairing code and wait
        run_host_mode(config, auth, session_manager).await?;
    }

    Ok(())
}

/// Run in host mode - generate pairing code and wait for clients
async fn run_host_mode(
    config: DaemonConfig,
    auth: AuthManager,
    session_manager: Arc<SessionManager>,
) -> Result<()> {
    // Generate pairing code
    let pairing_code = auth.generate_pairing_code(300).await; // 5 minutes
    tracing::info!("Pairing code: {}", pairing_code);
    println!("\n=== P2P Claude Code Daemon ===");
    println!("Pairing code: {}", pairing_code);
    println!("Valid for 5 minutes\n");

    // Create WebRTC connection
    let webrtc = WebRtcConnection::new_host(config.stun_servers)
        .await
        .context("Failed to create WebRTC connection")?;

    // Create SDP offer
    let offer_sdp = webrtc.create_offer().await
        .context("Failed to create offer")?;

    // Connect to signaling server
    let mut signaling = SignalingClient::connect(&config.signaling_url)
        .await
        .context("Failed to connect to signaling server")?;

    // Initialize and get our client ID
    let client_id = signaling.init().await
        .context("Failed to initialize signaling")?;
    tracing::info!("Signaling client ID: {}", client_id);

    // Send offer via signaling
    signaling.send_offer(offer_sdp).await
        .context("Failed to send offer")?;

    tracing::info!("Waiting for answer...");

    // Wait for answer
    let answer_sdp = signaling.recv_answer().await
        .context("Failed to receive answer")?;

    // Set remote description
    webrtc.set_answer(answer_sdp).await
        .context("Failed to set answer")?;

    // Wait for connection
    webrtc.wait_connected(30).await
        .context("Connection timeout")?;

    tracing::info!("WebRTC connection established!");

    // Main event loop
    run_event_loop(webrtc, session_manager).await
}

/// Run in client mode - connect to host with pairing code
async fn run_client_mode(
    config: DaemonConfig,
    _auth: AuthManager,
    session_manager: Arc<SessionManager>,
    pairing_code: &str,
) -> Result<()> {
    tracing::info!("Connecting with pairing code: {}", pairing_code);

    // Create WebRTC connection
    let webrtc = WebRtcConnection::new_host(config.stun_servers)
        .await
        .context("Failed to create WebRTC connection")?;

    // Connect to signaling server
    let mut signaling = SignalingClient::connect(&config.signaling_url)
        .await
        .context("Failed to connect to signaling server")?;

    // Initialize
    let _client_id = signaling.init().await
        .context("Failed to initialize signaling")?;

    // Send pair request
    signaling.pair(pairing_code).await
        .context("Failed to pair")?;

    tracing::info!("Paired! Waiting for offer...");

    // Wait for offer
    let offer_sdp = signaling.recv_offer().await
        .context("Failed to receive offer")?;

    // Set remote description
    webrtc.set_answer(offer_sdp).await
        .context("Failed to set answer")?;

    // Wait for connection
    webrtc.wait_connected(30).await
        .context("Connection timeout")?;

    tracing::info!("WebRTC connection established!");

    // Main event loop
    run_event_loop(webrtc, session_manager).await
}

/// Main event loop for handling messages
async fn run_event_loop(
    webrtc: WebRtcConnection,
    session_manager: Arc<SessionManager>,
) -> Result<()> {
    tracing::info!("Starting event loop...");

    // Add client to session manager
    let client_id = "webrtc-client".to_string();
    let _rx = session_manager.add_client(client_id.clone());

    loop {
        tokio::select! {
            // Receive from WebRTC data channel
            msg = webrtc.recv() => {
                if let Some(data) = msg {
                    match handle_incoming_message(&data, &session_manager, &client_id).await {
                        Ok(_) => {}
                        Err(e) => {
                            tracing::error!("Error handling message: {}", e);
                        }
                    }
                } else {
                    tracing::warn!("WebRTC connection closed");
                    break;
                }
            }

            // TODO: Broadcast from session manager
            // _ = async {
            //     while let Ok(msg) = rx.recv().await {
            //         // Send via WebRTC
            //     }
            // } => {}
        }
    }

    Ok(())
}

/// Handle incoming message from client
async fn handle_incoming_message(
    data: &Bytes,
    session_manager: &SessionManager,
    client_id: &str,
) -> Result<()> {
    // Parse message
    let msg: ClientMessage = serde_json::from_slice(data)
        .context("Failed to parse message")?;

    tracing::debug!("Received message: {:?}", msg);

    // Route to session manager
    if let Err(e) = session_manager.route_message(client_id, msg).await {
        tracing::error!("Error routing message: {}", e);
    }

    Ok(())
}
