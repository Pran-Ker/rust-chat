# Rust P2P Encrypted Chat

A peer-to-peer encrypted chat application that works over local WiFi networks. Share text messages, images, and videos securely with zero logging on your device.

## Features

- **P2P Discovery**: Automatically discovers peers on the same WiFi network using mDNS
- **End-to-End Encryption**: All messages encrypted with ChaCha20-Poly1305
- **Multi-Format Support**: Send text, images, and videos
- **Zero Logging**: No chat history stored on disk - all in memory
- **Multiple Peers**: Connect to multiple peers simultaneously
- **Privacy First**: No server, no logs, no traces

## Requirements

- Rust 1.91.1 or later
- Same WiFi network for all participants
- Firewall configured to allow the application

## Installation

```bash
cargo build --release
```

The binary will be located at `target/release/rust-chat`

## Usage

### Start the application

```bash
./target/release/rust-chat --name YourName
```

Or with a specific port:

```bash
./target/release/rust-chat --name YourName --port 50000
```

### Commands

Once running, use these commands:

- **Send text**: Just type your message and press Enter
- **Send image**: `/img /path/to/image.jpg`
- **Send video**: `/vid /path/to/video.mp4`
- **List peers**: `/peers`
- **Exit**: `/quit`

### Example Session

```bash
# Terminal 1 (Alice)
./target/release/rust-chat --name Alice

# Terminal 2 (Bob)
./target/release/rust-chat --name Bob

# Alice and Bob will automatically discover each other
# Start chatting!
```

## Security Features

1. **ChaCha20-Poly1305 Encryption**: Each peer generates a unique 256-bit encryption key
2. **No Disk Storage**: All messages exist only in memory
3. **Ephemeral Keys**: Encryption keys are generated per session
4. **Local Network Only**: Works only on your local WiFi network

## Privacy Guarantee

This application:
- Does NOT save any messages to disk
- Does NOT log any communication
- Does NOT connect to external servers
- Does NOT leave traces on your system after closing

## How It Works

1. **Discovery**: Each instance broadcasts its presence via mDNS on the local network
2. **Connection**: Peers connect directly to each other over TCP
3. **Encryption**: All data is encrypted before transmission using ChaCha20-Poly1305
4. **Broadcasting**: Messages are sent to all discovered peers

## Technical Details

- **Language**: Rust
- **Async Runtime**: Tokio
- **Discovery**: mDNS-SD (Multicast DNS Service Discovery)
- **Encryption**: ChaCha20-Poly1305 AEAD
- **Serialization**: Bincode
- **Max File Size**: 50MB (configurable in code)

## Limitations

- Same WiFi network required
- No message persistence
- No message history after restart
- File size limited to 50MB by default
- No delivery confirmation

## Building from Source

```bash
git clone <repo-url>
cd rust-chat
cargo build --release
```

## License

This project is provided as-is for educational and personal use.

## Warning

This is a privacy-focused chat application. While it uses strong encryption, it has not been audited by security professionals. Use at your own risk for sensitive communications.
