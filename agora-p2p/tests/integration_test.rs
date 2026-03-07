// agora-p2p/tests/integration_test.rs

use agora_p2p::{P2pNode, MeshEvent};
use agora_p2p::P2pConfig;
use agora_crypto::AgentId;
use tokio::time::{timeout, Duration};

fn make_agent_id(hex: &str) -> AgentId {
    AgentId::from_hex(hex).unwrap()
}

fn init_rustls_provider() {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_nodes_connect_and_exchange_message() {
    init_rustls_provider();
    // Two distinct agent IDs — deterministic initiator rule means the lexicographically
    // smaller one will initiate the connection.
    let id_a = make_agent_id(
        "0000000000000000000000000000000000000000000000000000000000000001",
    );
    let id_b = make_agent_id(
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    );

    let config_a = P2pConfig {
        identity_source: agora_p2p::IdentitySource::Testing(id_a.clone()),
        agent_id: id_a.clone(),
        listen_port: 0,
        service_name: "_agora-test._udp.local.".to_string(),
        transport: agora_p2p::TransportMode::Auto,
        wan_discovery: agora_p2p::WanDiscoveryMode::Disabled,
    };
    let config_b = P2pConfig {
        identity_source: agora_p2p::IdentitySource::Testing(id_b.clone()),
        agent_id: id_b.clone(),
        listen_port: 0,
        service_name: "_agora-test._udp.local.".to_string(),
        transport: agora_p2p::TransportMode::Auto,
        wan_discovery: agora_p2p::WanDiscoveryMode::Disabled,
    };

    let mut node_a = P2pNode::new(config_a).await.expect("node_a creation failed");
    let mut node_b = P2pNode::new(config_b).await.expect("node_b creation failed");

    let mut events_a = node_a.take_mesh_events().unwrap();
    let mut events_b = node_b.take_mesh_events().unwrap();

    // Start both nodes (port 0 = OS assigns)
    node_a.start(0).await.expect("node_a start failed");
    node_b.start(0).await.expect("node_b start failed");

    // Wait for connection events (mDNS discovery takes a moment)
    let connected_a = timeout(Duration::from_secs(10), async {
        loop {
            if let Some(event) = events_a.recv().await {
                if matches!(event, MeshEvent::Connected(_)) {
                    return true;
                }
            }
        }
    })
    .await
    .expect("node_a did not connect within 10s");

    assert!(connected_a, "node_a should have connected");

    // Send a message from A to B
    let peers_a = node_a.connected_peers().await;
    assert!(!peers_a.is_empty(), "node_a should have at least one connected peer");

    node_a
        .broadcast_room_message("test-room", b"hello from A")
        .await
        .expect("broadcast failed");

    // Wait for B to receive the message
    let received = timeout(Duration::from_secs(5), async {
        loop {
            if let Some(event) = events_b.recv().await {
                if matches!(event, MeshEvent::MessageReceived(_, _)) {
                    return true;
                }
            }
        }
    })
    .await
    .expect("node_b did not receive message within 5s");

    assert!(received, "node_b should have received a message from A");
}
