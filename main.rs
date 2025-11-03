// 1. CLI
// 2. Receive/Send messages using stdin/stdout
// 3. Use terminal raw mode to fix some bugs
//
//

use std::io::{BufRead, BufReader, Write};
use std::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use clap::{Parser, Subcommand, arg, command};
use colored::Colorize;

#[derive(Parser)]
#[command(name = "chat")]
#[command(about = "A simple TCP chat application", long_about = None)]
struct Cli {
    /// Your display name
    #[arg(value_name = "NAME")]
    name: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Connect to a server
    Connect {
        /// IP address to connect to
        #[arg(value_name = "HOST")]
        ip: String,

        /// Port number to connect to
        #[arg(value_name = "PORT")]
        port: u16,
    },
    /// Start a server
    Start {
        /// IP address to bind to
        #[arg(value_name = "HOST")]
        ip: String,

        /// Port number to bind to
        #[arg(value_name = "PORT")]
        port: u16,
    },
}

enum Event {
    RecvMessage(String),
    SendMessage(String),
    Exit,
}

fn get_formatted_time() -> String {
    let now = chrono::Local::now();
    now.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn print_with_time(msg: &str) {
    let time_str = get_formatted_time();
    println!("[{}] {}", time_str.dimmed(), msg);
}

fn run_event_loop(name: String, mut stream: TcpStream, recv: mpsc::Receiver<Event>) {
    loop {
        match recv.recv().expect("failed to receive event") {
            Event::Exit => {
                print_with_time("exiting...");
                break;
            }
            Event::RecvMessage(msg) => {
                print_with_time(&msg);
            }
            Event::SendMessage(msg) => {
                stream
                    .write_all(format!("{}: {}\n", name.blue(), msg).as_bytes())
                    .expect("failed to send message");

                print_with_time(&format!("{}: {}", name.green(), msg));
            }
        }
    }
}

/// Listen on addr:port, accept first connection
/// and return the TcpStream
fn start(addr: String, port: u16) -> TcpStream {
    println!("listening on {}:{}", addr, port);

    let listener = TcpListener::bind((addr, port)).expect("failed to bind to address");

    let (stream, client_addr) = listener.accept().expect("failed to accept connection");
    println!("client connected from {}", client_addr);

    stream
}

/// Connect to addr:port and return the TcpStream
fn connect(addr: String, port: u16) -> TcpStream {
    println!("connecting to {}:{}", addr, port);

    let stream = TcpStream::connect((addr.clone(), port)).expect("failed to connect to server");
    println!("connected to server at {}:{}", &addr, port);

    stream
}

fn start_message_listener(stream: TcpStream, event_tx: mpsc::Sender<Event>) {
    std::thread::spawn(move || {
        let reader = BufReader::new(stream);
        for msg in reader.lines() {
            let msg = msg.expect("failed to read message from stream");

            event_tx
                .send(Event::RecvMessage(msg))
                .expect("failed to send received message event");
        }

        event_tx
            .send(Event::Exit)
            .expect("failed to send exit event");
    });
}

fn start_input_listener(event_tx: mpsc::Sender<Event>) {
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let line = line.expect("failed to read line from stdin");
            if line == "exit" {
                break;
            }

            event_tx
                .send(Event::SendMessage(line))
                .expect("failed to send send message event");
        }

        event_tx
            .send(Event::Exit)
            .expect("failed to send exit event");
    });
}

fn main() {
    let cli = Cli::parse();
    let (event_tx, event_rx) = mpsc::channel::<Event>();

    let name = cli.name;
    let stream = match cli.command {
        Commands::Connect { ip, port } => connect(ip, port),
        Commands::Start { ip, port } => start(ip, port),
    };

    start_message_listener(
        stream.try_clone().expect("failed to clone"),
        event_tx.clone(),
    );
    start_input_listener(event_tx);

    run_event_loop(name, stream, event_rx);
}