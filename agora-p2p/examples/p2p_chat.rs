//! P2P Chat Example - Test LAN mesh between peers
//! 
//! Run on two machines on the same network:
//!   cargo run --example p2p_chat
//!
//! Or specify a port:
//!   cargo run --example p2p_chat -- --port 9000

use agora_p2p::{P2pNode, P2pConfig, MeshEvent};
use agora_crypto::AgentIdentity;
use rand::Rng;
use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize rustls crypto provider - required when both ring and aws-lc-rs are available
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    
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
    
    let config = P2pConfig {
        identity_source: agora_p2p::IdentitySource::default(),
        agent_id,
        listen_port: port,
        service_name: "_agora._udp.local.".to_string(),
        transport: agora_p2p::TransportMode::Auto,
        wan_discovery: agora_p2p::WanDiscoveryMode::Disabled,
    };
    
    let port = config.listen_port;
    let mut node = P2pNode::new(config).await?;
    node.start(port).await?;

    let mut node = node;
    let mut events = node.take_mesh_events()
        .expect("Failed to get mesh events");

    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                MeshEvent::Connected(peer_id) => {
                    println!("\n[Connected to {}]", peer_id);
                    print!("> ");
                    io::stdout().flush().ok();
                }
                MeshEvent::Disconnected(peer_id) => {
                    println!("\n[Disconnected from {}]", peer_id);
                    print!("> ");
                    io::stdout().flush().ok();
                }
                MeshEvent::MessageReceived(peer_id, amp_msg) => {
                    // Extract content bytes from EventPush messages
                    let msg = match &amp_msg {
                        agora_p2p::AmpMessage::EventPush { events, .. } => {
                            events.first()
                                .map(|e| String::from_utf8_lossy(&e.content).into_owned())
                                .unwrap_or_else(|| format!("{:?}", amp_msg))
                        }
                        other => format!("{:?}", other),
                    };
                    println!("\n[{}]: {}", &peer_id[..8], msg);
                    print!("> ");
                    io::stdout().flush().ok();
                }
                MeshEvent::Error(peer_id, err) => {
                    println!("\n[Error from {}]: {}", &peer_id[..8], err);
                    print!("> ");
                    io::stdout().flush().ok();
                }
            }
        }
    });

    let bound_addr = node.local_addr().await?;
    println!("Listening on port {}", bound_addr.port());
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
