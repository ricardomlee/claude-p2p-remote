# Getting Started

## Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- Node.js 18+ (for mobile app)
- Claude CLI (for daemon to interact with)
- (Optional) Mobile development tools for Tauri (Xcode for iOS, Android Studio for Android)

## Project Structure

```
claude-p2p-remote/
├── daemon/              # Rust daemon (WebRTC + Claude CLI)
│   ├── src/
│   │   ├── main.rs      # Entry point
│   │   ├── protocol/    # Message types
│   │   ├── webrtc/      # WebRTC connection
│   │   ├── session/     # Session management
│   │   ├── fs/          # File services
│   │   └── config/      # Configuration
│   └── Cargo.toml
├── signaling/           # WebSocket signaling server
│   ├── src/
│   │   └── main.rs      # Signaling server
│   └── Cargo.toml
├── mobile-app/          # Tauri v2 mobile app
│   ├── src/             # TypeScript frontend
│   ├── src-tauri/       # Rust backend
│   └── package.json
└── ARCHITECTURE.md      # Detailed architecture doc
```

## Quick Start

### 1. Start the Signaling Server

```bash
cd signaling
cargo run --release
```

The signaling server will start on `ws://localhost:8080/ws`.

### 2. Start the Daemon (Host Mode)

```bash
cd daemon
export ANTHROPIC_API_KEY="your-api-key"
cargo run --release -- --host
```

The daemon will:
- Spawn a Claude CLI process
- Generate a 6-digit pairing code
- Wait for mobile clients to connect

Example output:
```
=== P2P Claude Code Daemon ===
Pairing code: 123456
Valid for 5 minutes
```

### 3. Connect from Mobile App

#### Development Mode

```bash
cd mobile-app
npm install
npm run dev
```

Or build for production:

```bash
npm run build
npm run tauri build
```

#### Connect

1. Open the mobile app
2. Enter the 6-digit pairing code
3. Click "Connect"
4. Start chatting!

## Configuration

Create a `config.json` file in the daemon directory:

```json
{
  "api_key": "your-anthropic-api-key",
  "signaling_url": "ws://localhost:8080/ws",
  "stun_servers": ["stun.l.google.com:19302"],
  "root_path": ".",
  "confirm_mode": "auto",
  "listen_port": 8081
}
```

### Environment Variables

- `ANTHROPIC_API_KEY`: Your Anthropic API key
- `RUST_LOG`: Log level (e.g., `info`, `debug`, `trace`)
- `CLAUDE_P2P_CONFIG`: Path to config file

## Command Line Options

### Daemon

```
Usage: p2p-claude-daemon [OPTIONS]

Options:
  -c, --config <CONFIG>     Configuration file path [default: config.json]
      --confirm-mode <MODE> Confirmation mode: auto or manual [default: auto]
      --signaling <URL>     Signaling server URL
      --host                Run in host mode (wait for clients)
      --pair <CODE>         Connect as client with pairing code
  -h, --help                Print help
  -V, --version             Print version
```

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed architecture documentation.

### Key Components

1. **WebRTC P2P Connection**: Direct device-to-device communication
2. **Signaling Server**: Minimal WebSocket server for SDP exchange only
3. **Claude PTY Wrapper**: Interacts with Claude CLI via pseudo-terminal
4. **Session Manager**: Multiplexes multiple clients to single Claude process
5. **Confirmation Mode**: Auto-approve or manual approval for actions

### Connection Flow

```
1. Daemon starts, generates pairing code
2. Mobile app enters pairing code
3. Signaling server validates code, pairs clients
4. WebRTC offer/answer exchanged via signaling
5. P2P connection established (direct, no relay)
6. Messages flow over WebRTC DataChannel
```

## Development

### Build Daemon

```bash
cd daemon
cargo build --release
```

### Build Signaling Server

```bash
cd signaling
cargo build --release
```

### Build Mobile App

```bash
cd mobile-app
npm install
npm run tauri build
```

## Testing

### Test WebRTC Connection

```bash
# Terminal 1: Start signaling
cd signaling && cargo run

# Terminal 2: Start daemon in host mode
cd daemon && cargo run -- --host

# Terminal 3: Start daemon in client mode
cd daemon && cargo run -- --pair 123456
```

## Troubleshooting

### Connection Issues

1. Check signaling server is running: `curl http://localhost:8080/health`
2. Verify pairing code is correct (6 digits, not expired)
3. Check firewall settings for WebRTC ports
4. Try different STUN server if behind NAT

### Claude CLI Issues

1. Ensure Claude CLI is installed: `claude --version`
2. Verify API key is valid: `echo $ANTHROPIC_API_KEY`
3. Check Claude CLI works standalone first

## License

MIT
