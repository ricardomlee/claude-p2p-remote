# P2P Claude Code Remote

通过移动端 App (Tauri v2) 远程连接电脑/服务器上的 Claude Code，实现移动编码辅助。

## 核心特性

- **P2P 连接**: WebRTC DataChannel，无需中继服务器
- **混合信令**: WebSocket 仅用于 SDP 交换
- **多模态支持**: 语音、图片、视频输入（WebRTC 原生支持）
- **会话复用**: 多客户端共享一个 Claude 进程
- **确认模式**: 可配置（启动时选择 + 运行时切换）

## 技术栈

| 组件 | 技术 |
|------|------|
| Daemon | Rust + tokio |
| WebRTC | webrtc-rs (0.20.0-alpha.1) |
| 信令 | tokio-tungstenite (0.29.0) |
| PTY | shellwords + tokio::process |
| 移动端 | Tauri v2 (2.10.3) |
| 并发 | dashmap (6.1.0) |
| 序列化 | serde + serde_json |
| 日志 | tracing |

## 架构概览

```
┌─────────────────────────────────────────┐              ┌─────────────────┐
│     移动端 App (Tauri v2 + WebRTC)      │              │   电脑/服务器   │
│                                         │              │   (Host)        │
│  ┌───────────┐  ┌───────────────────┐   │              │                 │
│  │ WebRTC    │  │ UI 界面           │   │              │  ┌───────────┐  │
│  │ DataChan  │  │ - 对话 (文字/语音)│   │              │  │ Daemon    │  │
│  │ + Media   │  │ - 文件浏览        │   │              │  │ (Rust)    │  │
│  │           │  │ - 图片/视频预览   │   │              │  │           │  │
│  └───────────┘  └───────────────────┘   │              │  │  ┌─────┐  │  │
│                │                         │              │  │  │PTY  │  │  │
│  ┌───────────┐  │                         │              │  │  │     │  │  │
│  │ 媒体输入  │  │                         │              │  │  │Claude│  │  │
│  │ - 麦克风  │──┘                         │              │  │  └─────┘  │  │
│  │ - 相机    │                            │              │  └───────────┘  │
│  └───────────┘                            │              └─────────────────┘
└───────────────────────────────────────────┘
```

## 项目结构

```
.
├── daemon/          # Rust Daemon (WebRTC + Claude PTY)
├── signaling/       # 信令服务器 (WebSocket)
├── mobile-app/      # Tauri v2 移动端应用
└── ARCHITECTURE.md  # 详细架构设计文档
```

## 状态

🚧 **设计阶段** - 架构设计已完成，准备开始实现

## License

MIT
