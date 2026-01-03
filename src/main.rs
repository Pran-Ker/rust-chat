use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use clap::{Parser, arg, command};
use colored::Colorize;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

const SERVICE_TYPE: &str = "_rustchat._tcp.local.";
const NONCE_SIZE: usize = 12;
const MAX_MESSAGE_SIZE: usize = 50 * 1024 * 1024; // 50MB max for videos

#[derive(Parser)]
#[command(name = "rust-chat")]
#[command(about = "P2P Encrypted Chat - Share text, images, and videos securely over WiFi", long_about = None)]
struct Cli {
    /// Your display name
    #[arg(short, long, value_name = "NAME")]
    name: String,

    /// Port to listen on (default: random)
    #[arg(short, long, value_name = "PORT")]
    port: Option<u16>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
enum MessageType {
    Text(String),
    Image { filename: String, data: Vec<u8> },
    Video { filename: String, data: Vec<u8> },
    KeyExchange { public_key: Vec<u8> },
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    sender: String,
    msg_type: MessageType,
    timestamp: i64,
}

#[derive(Clone)]
struct Peer {
    name: String,
    addr: SocketAddr,
}

struct ChatApp {
    name: String,
    port: u16,
    peers: Arc<Mutex<HashMap<SocketAddr, Peer>>>,
    encryption_key: Vec<u8>,
}

impl ChatApp {
    fn new(name: String, port: u16) -> Self {
        let mut key = vec![0u8; 32];
        rand::thread_rng().fill(&mut key[..]);

        Self {
            name,
            port,
            peers: Arc::new(Mutex::new(HashMap::new())),
            encryption_key: key,
        }
    }

    fn encrypt_message(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.encryption_key)
            .map_err(|e| format!("Cipher init error: {}", e))?;

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::thread_rng().fill(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| format!("Encryption error: {}", e))?;

        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    async fn start_mdns_discovery(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mdns = ServiceDaemon::new()?;
        let service_name = format!("{}-{}", self.name, rand::thread_rng().r#gen::<u32>());

        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            &service_name,
            &format!("{}.local.", service_name),
            "",
            self.port,
            None,
        )?;

        mdns.register(service_info)?;

        let receiver = mdns.browse(SERVICE_TYPE)?;
        let peers = self.peers.clone();
        let my_port = self.port;

        tokio::spawn(async move {
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        if let Some(addr) = info.get_addresses().iter().next() {
                            let port = info.get_port();

                            if port != my_port {
                                let socket_addr = SocketAddr::new(*addr, port);
                                let peer_name = info
                                    .get_fullname()
                                    .split('.')
                                    .next()
                                    .unwrap_or("Unknown")
                                    .to_string();

                                let mut peers_lock = peers.lock().await;
                                if !peers_lock.contains_key(&socket_addr) {
                                    peers_lock.insert(
                                        socket_addr,
                                        Peer {
                                            name: peer_name.clone(),
                                            addr: socket_addr,
                                        },
                                    );
                                    drop(peers_lock);

                                    println!(
                                        "{} {}",
                                        "Discovered peer:".green(),
                                        peer_name.blue()
                                    );
                                }
                            }
                        }
                    }
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        let peer_name = fullname.split('.').next().unwrap_or("Unknown");
                        println!("{} {}", "Peer left:".red(), peer_name.blue());
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }

    async fn start_listener(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port)).await?;
        let peers = self.peers.clone();
        let encryption_key = self.encryption_key.clone();
        let name = self.name.clone();

        tokio::spawn(async move {
            loop {
                if let Ok((socket, addr)) = listener.accept().await {
                    let peers = peers.clone();
                    let encryption_key = encryption_key.clone();
                    let name = name.clone();

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(
                            socket,
                            addr,
                            peers,
                            encryption_key,
                            name,
                        )
                        .await
                        {
                            eprintln!("Connection error: {}", e);
                        }
                    });
                }
            }
        });

        Ok(())
    }

    async fn handle_connection(
        mut socket: TcpStream,
        _addr: SocketAddr,
        peers: Arc<Mutex<HashMap<SocketAddr, Peer>>>,
        encryption_key: Vec<u8>,
        _my_name: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cipher = ChaCha20Poly1305::new_from_slice(&encryption_key)
            .map_err(|e| format!("Cipher init error: {}", e))?;

        loop {
            let len = match socket.read_u32().await {
                Ok(l) => l as usize,
                Err(_) => break,
            };

            if len == 0 || len > MAX_MESSAGE_SIZE {
                break;
            }

            let mut encrypted_data = vec![0u8; len];
            socket.read_exact(&mut encrypted_data).await?;

            if encrypted_data.len() < NONCE_SIZE {
                continue;
            }

            let nonce = Nonce::from_slice(&encrypted_data[..NONCE_SIZE]);
            let ciphertext = &encrypted_data[NONCE_SIZE..];

            if let Ok(decrypted) = cipher.decrypt(nonce, ciphertext) {
                if let Ok(message) = bincode::deserialize::<Message>(&decrypted) {
                    Self::display_message(&message, &peers).await;
                }
            }
        }

        Ok(())
    }

    async fn display_message(message: &Message, _peers: &Arc<Mutex<HashMap<SocketAddr, Peer>>>) {
        let time_str = chrono::DateTime::from_timestamp(message.timestamp, 0)
            .map(|dt| dt.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "??:??:??".to_string());

        match &message.msg_type {
            MessageType::Text(text) => {
                println!(
                    "[{}] {}: {}",
                    time_str.dimmed(),
                    message.sender.blue(),
                    text
                );
            }
            MessageType::Image { filename, data } => {
                println!(
                    "[{}] {} sent image: {} ({} KB)",
                    time_str.dimmed(),
                    message.sender.blue(),
                    filename.yellow(),
                    data.len() / 1024
                );
            }
            MessageType::Video { filename, data } => {
                println!(
                    "[{}] {} sent video: {} ({} MB)",
                    time_str.dimmed(),
                    message.sender.blue(),
                    filename.yellow(),
                    data.len() / (1024 * 1024)
                );
            }
            MessageType::KeyExchange { .. } => {}
        }
    }

    async fn send_to_peer(
        &self,
        addr: SocketAddr,
        message: &Message,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut stream = TcpStream::connect(addr).await?;

        let serialized = bincode::serialize(message)?;
        let encrypted = self.encrypt_message(&serialized)?;

        stream.write_u32(encrypted.len() as u32).await?;
        stream.write_all(&encrypted).await?;
        stream.flush().await?;

        Ok(())
    }

    async fn broadcast_message(&self, msg_type: MessageType) {
        let message = Message {
            sender: self.name.clone(),
            msg_type: msg_type.clone(),
            timestamp: chrono::Utc::now().timestamp(),
        };

        let peers = self.peers.lock().await;
        let peer_addrs: Vec<SocketAddr> = peers.keys().copied().collect();
        drop(peers);

        for addr in peer_addrs {
            if let Err(e) = self.send_to_peer(addr, &message).await {
                eprintln!("Failed to send to {}: {}", addr, e);
            }
        }

        Self::display_message(&message, &self.peers).await;
    }

    async fn handle_input(&self) {
        use tokio::io::{AsyncBufReadExt, BufReader};

        println!("\n{}", "Commands:".green().bold());
        println!("  {}  - Send a text message", "text <message>".cyan());
        println!("  {} - Send an image file", "/img <filepath>".cyan());
        println!("  {} - Send a video file", "/vid <filepath>".cyan());
        println!("  {}        - List connected peers", "/peers".cyan());
        println!("  {}         - Exit the chat", "/quit".cyan());
        println!();

        let stdin = tokio::io::stdin();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        loop {
            print!("> ");
            std::io::Write::flush(&mut std::io::stdout()).unwrap();

            match lines.next_line().await {
                Ok(Some(input)) => {
                    let input = input.trim();

                    if input.starts_with("/quit") {
                        break;
                    } else if input.starts_with("/peers") {
                        self.list_peers().await;
                    } else if input.starts_with("/img ") {
                        let path = input.strip_prefix("/img ").unwrap().trim();
                        self.send_file(path, true).await;
                    } else if input.starts_with("/vid ") {
                        let path = input.strip_prefix("/vid ").unwrap().trim();
                        self.send_file(path, false).await;
                    } else if !input.is_empty() {
                        self.broadcast_message(MessageType::Text(input.to_string())).await;
                    }
                }
                Ok(None) | Err(_) => break,
            }
        }
    }

    async fn list_peers(&self) {
        let peers = self.peers.lock().await;
        if peers.is_empty() {
            println!("{}", "No peers connected yet.".yellow());
        } else {
            println!("\n{}", "Connected Peers:".green().bold());
            for peer in peers.values() {
                println!("  {} - {}", peer.name.blue(), peer.addr);
            }
            println!();
        }
    }

    async fn send_file(&self, path: &str, is_image: bool) {
        match tokio::fs::read(path).await {
            Ok(data) => {
                let filename = std::path::Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let msg_type = if is_image {
                    MessageType::Image { filename, data }
                } else {
                    MessageType::Video { filename, data }
                };

                self.broadcast_message(msg_type).await;
            }
            Err(e) => {
                eprintln!("{} {}", "Error reading file:".red(), e);
            }
        }
    }

    async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", "=".repeat(60).green());
        println!(
            "{}",
            "  P2P Encrypted Chat - Secure Local Network Messaging".green().bold()
        );
        println!("{}", "=".repeat(60).green());
        println!();
        println!("{}  {}", "Your name:".cyan(), self.name.blue().bold());
        println!("{}      {}", "Port:".cyan(), self.port);
        println!();
        println!("{}", "Starting services...".yellow());

        self.start_listener().await?;
        println!("{}", "  Listening for connections".green());

        self.start_mdns_discovery().await?;
        println!("{}", "  Broadcasting presence on network".green());

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        println!();
        println!("{}", "Ready! Waiting for peers...".green().bold());

        self.handle_input().await;

        println!("\n{}", "Goodbye!".cyan());
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let port = cli.port.unwrap_or_else(|| {
        let mut rng = rand::thread_rng();
        rng.gen_range(49152..65535)
    });

    let app = ChatApp::new(cli.name, port);
    app.run().await?;

    Ok(())
}
