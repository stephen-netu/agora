#![warn(
    missing_docs,
    rust_2018_idioms,
    unused_import_braces,
    unused_qualifications,
    clippy::all,
    clippy::pedantic,
)]
#![allow(
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
)]

//! Agora CLI - Command-line interface for the Agora communications platform.

mod client;
mod sigchain;
mod tui;

use clap::{Parser, Subcommand};
use client::AgoraClient;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "agora", about = "Agora communications platform CLI")]
struct Cli {
    /// Server URL (default: http://localhost:8008)
    #[arg(long, default_value = "http://localhost:8008", global = true)]
    server: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Register a new account
    Register {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        password: String,
    },
    /// Log in and save the access token
    Login {
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        password: String,
    },
    /// Log out and clear saved token
    Logout,
    /// Room management
    #[command(subcommand)]
    Rooms(RoomCommands),
    /// Space management
    #[command(subcommand)]
    Spaces(SpaceCommands),
    /// Send a message to a room
    Send {
        /// Room ID (e.g. !abc123:localhost)
        #[arg(short, long)]
        room: String,
        /// Message text
        message: Vec<String>,
    },
    /// Show recent messages in a room
    Messages {
        /// Room ID
        #[arg(short, long)]
        room: String,
        /// Number of messages to show
        #[arg(short, long, default_value = "20")]
        limit: u64,
    },
    /// Upload a file and get its mxc:// URI
    Upload {
        /// Path to the file to upload
        file: PathBuf,
    },
    /// Download media by mxc:// URI
    Download {
        /// mxc:// URI to download
        uri: String,
        /// Destination path (defaults to original filename or "download")
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Launch interactive TUI
    Connect,
    /// Show this device's sigchain Agent ID
    AgentId,
}

#[derive(Subcommand)]
enum RoomCommands {
    /// Create a new room
    Create {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        topic: Option<String>,
    },
    /// Join an existing room
    Join {
        /// Room ID to join
        room: String,
    },
    /// Leave a room
    Leave {
        /// Room ID to leave
        room: String,
    },
    /// List joined rooms (via initial sync)
    List,
}

#[derive(Subcommand)]
enum SpaceCommands {
    /// Create a new space
    Create {
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        topic: Option<String>,
    },
    /// Add a room as a child of a space
    AddChild {
        /// Space ID
        #[arg(short, long)]
        space: String,
        /// Room ID to add
        #[arg(short, long)]
        room: String,
    },
    /// Remove a room from a space
    RemoveChild {
        /// Space ID
        #[arg(short, long)]
        space: String,
        /// Room ID to remove
        #[arg(short, long)]
        room: String,
    },
    /// Show the room hierarchy under a space
    Hierarchy {
        /// Space ID
        space: String,
    },
    /// List joined spaces
    List,
}

fn token_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("agora")
        .join("token")
}

fn save_token(token: &str) -> std::io::Result<()> {
    let path = token_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, token)
}

fn load_token() -> Option<String> {
    std::fs::read_to_string(token_path()).ok().map(|s| s.trim().to_owned())
}

fn clear_token() {
    let _ = std::fs::remove_file(token_path());
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let mut client = AgoraClient::new(&cli.server);

    // Load saved token if available.
    if let Some(token) = load_token() {
        client.set_token(token);
    }

    let result = run(cli.command, &mut client).await;
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run(
    command: Commands,
    client: &mut AgoraClient,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Commands::Register { username, password } => {
            let resp = client.register(&username, &password).await?;
            save_token(&resp.access_token)?;
            println!("registered as {}", resp.user_id);
            println!("device_id: {}", resp.device_id);
            println!("token saved");
        }

        Commands::Login { username, password } => {
            let resp = client.login(&username, &password).await?;
            save_token(&resp.access_token)?;
            println!("logged in as {}", resp.user_id);
            println!("device_id: {}", resp.device_id);
            println!("token saved");
        }

        Commands::Logout => {
            client.logout().await?;
            clear_token();
            println!("logged out");
        }

        Commands::Rooms(sub) => match sub {
            RoomCommands::Create { name, topic } => {
                let resp = client
                    .create_room(name.as_deref(), topic.as_deref())
                    .await?;
                println!("created room: {}", resp.room_id);
            }
            RoomCommands::Join { room } => {
                let resp = client.join_room(&room).await?;
                println!("joined room: {}", resp.room_id);
            }
            RoomCommands::Leave { room } => {
                client.leave_room(&room).await?;
                println!("left room: {}", room);
            }
            RoomCommands::List => {
                let resp = client.sync(None, 0).await?;
                if resp.rooms.join.is_empty() {
                    println!("no joined rooms");
                } else {
                    for (room_id, room) in &resp.rooms.join {
                        let name = room
                            .state
                            .events
                            .iter()
                            .find(|e| e.event_type == "m.room.name")
                            .and_then(|e| e.content.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("(unnamed)");
                        println!("  {room_id}  {name}");
                    }
                }
            }
        },

        Commands::Spaces(sub) => match sub {
            SpaceCommands::Create { name, topic } => {
                let resp = client
                    .create_space(name.as_deref(), topic.as_deref())
                    .await?;
                println!("created space: {}", resp.room_id);
            }
            SpaceCommands::AddChild { space, room } => {
                client
                    .set_state_event(
                        &space,
                        "m.space.child",
                        &room,
                        serde_json::json!({ "via": [client.server_name()] }),
                    )
                    .await?;
                println!("added {} to space {}", room, space);
            }
            SpaceCommands::RemoveChild { space, room } => {
                client
                    .set_state_event(&space, "m.space.child", &room, serde_json::json!({}))
                    .await?;
                println!("removed {} from space {}", room, space);
            }
            SpaceCommands::Hierarchy { space } => {
                let resp = client.get_hierarchy(&space).await?;
                if resp.rooms.is_empty() {
                    println!("empty space");
                } else {
                    for room in &resp.rooms {
                        let kind = if room.room_type.as_deref() == Some("m.space") {
                            "[space]"
                        } else {
                            "[room]"
                        };
                        let name = room.name.as_deref().unwrap_or("(unnamed)");
                        println!(
                            "  {} {} {} ({} members)",
                            kind, room.room_id, name, room.num_joined_members
                        );
                    }
                }
            }
            SpaceCommands::List => {
                let resp = client.sync(None, 0).await?;
                let mut found = false;
                for (room_id, room) in &resp.rooms.join {
                    let is_space = room
                        .state
                        .events
                        .iter()
                        .find(|e| e.event_type == "m.room.create")
                        .and_then(|e| e.content.get("type"))
                        .and_then(|v| v.as_str())
                        == Some("m.space");
                    if is_space {
                        let name = room
                            .state
                            .events
                            .iter()
                            .find(|e| e.event_type == "m.room.name")
                            .and_then(|e| e.content.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("(unnamed)");
                        println!("  {room_id}  {name}");
                        found = true;
                    }
                }
                if !found {
                    println!("no joined spaces");
                }
            }
        },

        Commands::Send { room, message } => {
            let body = message.join(" ");

            // Attempt to attach a sigchain Action link to the message.
            // Failure is non-fatal: the message is sent regardless, with a
            // warning emitted to stderr.
            let content_base = serde_json::json!({
                "msgtype": "m.text",
                "body": body,
            });

            // correlation_path is empty for top-level user messages (S-05).
            let sigchain_proof = try_publish_action(
                client,
                &room,
                "m.room.message",
                &content_base,
                vec![],
            )
            .await;

            // Include the sigchain_proof in the event content if available.
            let content = if let Some(ref proof) = sigchain_proof {
                let mut c = content_base.clone();
                c.as_object_mut().unwrap().insert(
                    "sigchain_proof".into(),
                    serde_json::to_value(proof).unwrap_or(serde_json::Value::Null),
                );
                c
            } else {
                content_base
            };

            let resp = client.send_event(&room, "m.room.message", content).await?;
            if let Some(ref proof) = sigchain_proof {
                println!("sent: {} [sigchain seqno={}]", resp.event_id, proof.seqno);
            } else {
                println!("sent: {}", resp.event_id);
            }
        }

        Commands::Messages { room, limit } => {
            let resp = client.get_messages(&room, limit).await?;
            if resp.chunk.is_empty() {
                println!("no messages");
            } else {
                for event in resp.chunk.iter().rev() {
                    let sender = event.sender.localpart();
                    let fallback = format!("[{}]", event.event_type);
                    let body = event
                        .content
                        .get("body")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&fallback);
                    println!("<{sender}> {body}");
                }
            }
        }

        Commands::Upload { file } => {
            let uri = client.upload_file(&file).await?;
            println!("{uri}");
        }

        Commands::Download { uri, output } => {
            let dest = output.unwrap_or_else(|| {
                let name = uri
                    .rsplit('/')
                    .next()
                    .unwrap_or("download")
                    .to_owned();
                PathBuf::from(if name.is_empty() { "download" } else { &name })
            });
            client.download_file(&uri, &dest).await?;
            println!("saved to {}", dest.display());
        }

        Commands::Connect => {
            tui::run_tui(client).await?;
        }

        Commands::AgentId => {
            let config_dir = dirs::config_dir()
                .ok_or("cannot locate config dir")?
                .join("agora");
            let manager = sigchain::SigchainManager::open(&config_dir)
                .map_err(|e| format!("sigchain: {e}"))?;
            println!("{}", manager.agent_id_hex());
        }
    }

    Ok(())
}

// ── Sigchain helpers ──────────────────────────────────────────────────────────

/// Metadata included in the event content to link it to a sigchain Action link.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct SigchainProof {
    /// Sequence number of the corresponding Action sigchain link.
    pub seqno: u64,
    /// Full hex-encoded `AgentId` of the signing agent.
    pub agent_id: String,
}

/// Attempt to append and publish a sigchain `Action` link for an outgoing event.
///
/// `correlation_path` is the caller-supplied ancestry chain (empty for top-level
/// user actions). If this agent's `AgentId` appears in the path, a `Refusal`
/// link is appended and published instead, and `None` is returned with a
/// warning. This enforces the S-05 loop-detection protocol.
///
/// Returns `Some(SigchainProof)` on success, `None` on any error.
pub(crate) async fn try_publish_action(
    client: &AgoraClient,
    room_id: &str,
    event_type: &str,
    content: &serde_json::Value,
    correlation_path: Vec<agora_crypto::AgentId>,
) -> Option<SigchainProof> {
    let config_dir = dirs::config_dir()?.join("agora");

    let mut manager = sigchain::SigchainManager::open(&config_dir)
        .map_err(|e| eprintln!("sigchain: open failed: {e}"))
        .ok()?;

    let agent_id_hex = manager.agent_id_hex();

    // S-05 loop detection: refuse if our AgentId is already in the path.
    if manager.has_loop(&correlation_path) {
        eprintln!(
            "sigchain: loop detected in correlation_path — appending Refusal for {event_type}"
        );
        let refusal = manager
            .append_refusal(event_type, correlation_path)
            .map_err(|e| eprintln!("sigchain: append_refusal failed: {e}"))
            .ok()?;

        // Best-effort publish — chain integrity is local; server is non-fatal.
        let _ = client.publish_sigchain_link(&agent_id_hex, &refusal).await;
        let _ = manager.save();
        return None;
    }

    let link = manager
        .append_action(event_type, room_id, content, correlation_path)
        .map_err(|e| eprintln!("sigchain: append_action failed: {e}"))
        .ok()?;

    let (seqno, _hash) = client
        .publish_sigchain_link(&agent_id_hex, &link)
        .await
        .map_err(|e| eprintln!("sigchain: publish failed (chain still updated locally): {e}"))
        .ok()?;

    // Persist even on publish failure — chain stays consistent locally.
    manager
        .save()
        .map_err(|e| eprintln!("sigchain: save failed: {e}"))
        .ok()?;

    Some(SigchainProof { seqno, agent_id: agent_id_hex })
}
