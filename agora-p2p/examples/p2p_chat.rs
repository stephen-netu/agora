//! P2P Chat Example - Test LAN mesh between peers
//! 
//! Run on two machines on the same network:
//!   cargo run --example p2p_chat
//!
//! Or specify a port:
//!   cargo run --example p2p_chat -- --port 9000

use agora_p2p::{P2pNode, Config};
use agora_crypto::AgentIdentity;
use rand::Rng;
use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let port = args.iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1).and_then(|p| p.parse().ok()))
        .unwrap_or(0);

    let mut seed = [0u8; 32];
    rand::thread_rng().fill(&mut seed);
    let identity = AgentIdentity::from_seed(&seed);
    let agent_id = identity.agent_id.clone();
    
    println!("Starting P2P node with AgentId: {}", agent_id.to_hex());
    
    let config = Config {
        agent_id,
        listen_port: port,
        service_name: "_agora._udp.local".to_string(),
    };
    
    let node = P2pNode::new(config).await?;
    node.start(port).await?;
    
    println!("Listening on port {}", port);
    println!("Waiting for peers on local network...");
    println!("Type a message and press Enter to broadcast to all connected peers.");
    println!("Type 'quit' to exit.");
    
    loop {
        let mut input = String::new();
        print!("> ");
        io::stdout().flush()?;
        
        match io::stdin().read_line(&mut input) {
            Ok(0) => break,
            Ok(_) => {
                let input = input.trim();
                if input == "quit" {
                    break;
                }
                if !input.is_empty() {
                    if let Err(e) = node.broadcast_room_message("lobby", input.as_bytes()).await {
                        eprintln!("Broadcast error: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Read error: {}", e);
                break;
            }
        }
    }
    
    println!("Shutting down...");
    Ok(())
}
