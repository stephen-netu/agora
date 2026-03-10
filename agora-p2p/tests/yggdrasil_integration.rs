// agora-p2p/tests/yggdrasil_integration.rs

#[cfg(feature = "yggdrasil-integration-tests")]
mod tests {
    use agora_p2p::{P2pNode, MeshEvent};
    use agora_p2p::types::{P2pConfig, TransportMode, YggdrasilConfig};
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
    async fn two_nodes_exchange_eventpush_over_yggdrasil() {
        init_rustls_provider();

        let id_a = make_agent_id(
            "0000000000000000000000000000000000000000000000000000000000000001",
        );
        let id_b = make_agent_id(
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        );

        let config_a = P2pConfig {
            agent_id: id_a.clone(),
            listen_port: 0,
            service_name: "_agora-ygg-test._udp.local.".to_string(),
            transport: TransportMode::Yggdrasil(YggdrasilConfig {
                admin_socket: None,
                listen_port: 0,
            }),
        };
        let config_b = P2pConfig {
            agent_id: id_b.clone(),
            listen_port: 0,
            service_name: "_agora-ygg-test._udp.local.".to_string(),
            transport: TransportMode::Yggdrasil(YggdrasilConfig {
                admin_socket: None,
                listen_port: 0,
            }),
        };

        let mut node_a = P2pNode::new(config_a).await.expect("node_a creation failed");
        let mut node_b = P2pNode::new(config_b).await.expect("node_b creation failed");

        let mut events_a = node_a.take_mesh_events().unwrap();
        let mut events_b = node_b.take_mesh_events().unwrap();

        node_a.start(0).await.expect("node_a start failed");
        node_b.start(0).await.expect("node_b start failed");

        let connected = timeout(Duration::from_secs(15), async {
            loop {
                if let Some(event) = events_a.recv().await {
                    if matches!(event, MeshEvent::Connected(_)) {
                        return true;
                    }
                }
            }
        })
        .await
        .expect("node_a did not connect to node_b within 15s");

        assert!(connected, "node_a should have connected to node_b");

        let peers_a = node_a.connected_peers().await;
        assert!(
            !peers_a.is_empty(),
            "node_a should have at least one connected peer"
        );

        node_a
            .broadcast_grove_message("test-room", b"hello via yggdrasil")
            .await
            .expect("broadcast from node_a failed");

        let received_b = timeout(Duration::from_secs(10), async {
            loop {
                if let Some(event) = events_b.recv().await {
                    if matches!(event, MeshEvent::MessageReceived(_, _)) {
                        return true;
                    }
                }
            }
        })
        .await
        .expect("node_b did not receive message within 10s");

        assert!(received_b, "node_b should have received a message from node_a");

        node_b
            .broadcast_grove_message("test-room", b"response via yggdrasil")
            .await
            .expect("broadcast from node_b failed");

        let received_a = timeout(Duration::from_secs(10), async {
            loop {
                if let Some(event) = events_a.recv().await {
                    if let MeshEvent::MessageReceived(peer_id, _) = event {
                        if peer_id == id_b.to_string() {
                            return true;
                        }
                    }
                }
            }
        })
        .await
        .expect("node_a did not receive message from node_b within 10s");

        assert!(
            received_a,
            "node_a should have received a message from node_b with correct AgentId"
        );
    }
}
