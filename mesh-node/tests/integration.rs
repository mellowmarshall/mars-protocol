//! Integration test: two nodes on localhost, publish + discover a capability.
//!
//! This is the Phase 0 demo from Section 12.2 of the protocol spec.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use mesh_core::frame::{
    MSG_FIND_NODE, MSG_FIND_NODE_RESULT, MSG_FIND_VALUE, MSG_FIND_VALUE_RESULT, MSG_PING, MSG_PONG,
    MSG_STORE, MSG_STORE_ACK,
};
use mesh_core::hash::schema_hash;
use mesh_core::identity::Keypair;
use mesh_core::message::{
    FindNode, FindValue, FindValueResult, NodeAddr, Ping, Pong, Store, StoreAck, from_cbor, to_cbor,
};
use mesh_core::routing::routing_key;
use mesh_core::{Descriptor, Frame};
use mesh_dht::DhtNode;
use mesh_dht::node::DhtConfig;
use mesh_transport::MeshEndpoint;
use tokio::sync::Mutex;

/// Helper: create a localhost address with OS-assigned port.
fn localhost() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
}

/// Helper: create an endpoint with a given keypair.
fn make_endpoint(kp: &Keypair) -> MeshEndpoint {
    MeshEndpoint::new(localhost(), kp).unwrap()
}

fn now_micros() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

fn make_node_addr(addr: &str) -> NodeAddr {
    NodeAddr {
        protocol: "quic".into(),
        address: addr.to_string(),
    }
}

/// Start a node's listener that dispatches requests to its DhtNode.
/// Returns the endpoint, the DhtNode (shared), and the listener task handle.
fn start_node(
    keypair: Keypair,
    endpoint: MeshEndpoint,
) -> (Arc<Mutex<DhtNode>>, tokio::task::JoinHandle<()>) {
    let local_addr = endpoint.local_addr().unwrap();
    let node_addr = make_node_addr(&local_addr.to_string());

    let dht = Arc::new(Mutex::new(DhtNode::new(
        keypair,
        node_addr,
        DhtConfig::default(),
    )));

    let dht_clone = dht.clone();
    let handle = tokio::spawn(async move {
        endpoint
            .listen(move |frame, sender, _peer_identity| {
                let dht = dht_clone.clone();
                async move {
                    let response = {
                        let mut node = dht.lock().await;
                        match frame.msg_type {
                            MSG_PING => {
                                let ping: Ping = from_cbor(&frame.body).unwrap();
                                let pong = node.handle_ping(&ping);
                                let body = to_cbor(&pong).unwrap();
                                Frame::response(&frame, MSG_PONG, body)
                            }
                            MSG_STORE => {
                                let store: Store = from_cbor(&frame.body).unwrap();
                                let ack = node.handle_store(&store);
                                let body = to_cbor(&ack).unwrap();
                                Frame::response(&frame, MSG_STORE_ACK, body)
                            }
                            MSG_FIND_NODE => {
                                let find: FindNode = from_cbor(&frame.body).unwrap();
                                let result = node.handle_find_node(&find);
                                let body = to_cbor(&result).unwrap();
                                Frame::response(&frame, MSG_FIND_NODE_RESULT, body)
                            }
                            MSG_FIND_VALUE => {
                                let find: FindValue = from_cbor(&frame.body).unwrap();
                                let result = node.handle_find_value(&find);
                                let body = to_cbor(&result).unwrap();
                                Frame::response(&frame, MSG_FIND_VALUE_RESULT, body)
                            }
                            other => {
                                panic!("unexpected msg_type: 0x{other:02x}");
                            }
                        }
                    };
                    sender.send(&response).await.unwrap();
                }
            })
            .await
            .unwrap();
    });

    (dht, handle)
}

#[tokio::test]
async fn end_to_end_publish_and_discover() {
    // ── Setup: two nodes on localhost ──

    let kp_a_secret = Keypair::generate().secret_bytes();
    let kp_a = Keypair::from_bytes(&kp_a_secret);
    let kp_b_secret = Keypair::generate().secret_bytes();
    let kp_b = Keypair::from_bytes(&kp_b_secret);

    let ep_a = make_endpoint(&Keypair::from_bytes(&kp_a_secret));
    let ep_b = make_endpoint(&Keypair::from_bytes(&kp_b_secret));

    let addr_a = ep_a.local_addr().unwrap();
    let _addr_b = ep_b.local_addr().unwrap();

    let (dht_a, handle_a) = start_node(Keypair::from_bytes(&kp_a_secret), ep_a);
    let (_dht_b, handle_b) = start_node(Keypair::from_bytes(&kp_b_secret), ep_b);

    // Give listeners a moment to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // ── Step 1: Node A publishes a capability descriptor ──

    let cap_type = "compute/inference/text-generation";
    let rk = routing_key(cap_type);
    let payload = b"test-capability-payload".to_vec();

    let descriptor = Descriptor::create(
        &kp_a,
        schema_hash("core/capability"),
        cap_type.to_string(),
        payload.clone(),
        now_micros(),
        1,
        3600,
        vec![rk.clone()],
    )
    .unwrap();

    let published_id = descriptor.id.clone();
    let published_publisher = descriptor.publisher.clone();

    // STORE the descriptor to Node A (self-store for simplicity)
    {
        let mut node_a = dht_a.lock().await;
        let store = Store {
            sender: kp_a.identity(),
            sender_addr: make_node_addr(&addr_a.to_string()),
            descriptor,
        };
        let ack = node_a.handle_store(&store);
        assert!(ack.stored, "store should succeed: {:?}", ack.reason);
    }

    // ── Step 2: Node B discovers it via FIND_VALUE over QUIC ──

    // Node B connects to Node A and sends FIND_VALUE
    let client_ep = make_endpoint(&kp_b);
    let conn_to_a = client_ep.connect(addr_a).await.unwrap();

    let find_value = FindValue {
        sender: kp_b.identity(),
        sender_addr: make_node_addr(&client_ep.local_addr().unwrap().to_string()),
        key: rk.clone(),
        max_results: 20,
        filters: None,
    };
    let body = to_cbor(&find_value).unwrap();
    let frame = Frame::new(MSG_FIND_VALUE, body);

    let response = mesh_transport::send_request(&conn_to_a, &frame)
        .await
        .unwrap();

    assert_eq!(response.msg_type, MSG_FIND_VALUE_RESULT);

    let result: FindValueResult = from_cbor(&response.body).unwrap();
    let descriptors = result.descriptors.expect("should have descriptors");

    // ── Step 3: Verify the returned descriptor matches ──

    assert_eq!(descriptors.len(), 1);
    let found = &descriptors[0];
    assert_eq!(found.id, published_id);
    assert_eq!(found.publisher, published_publisher);
    assert_eq!(found.topic, cap_type);
    assert_eq!(found.payload, payload);

    // ── Step 4: Also test PING over QUIC ──

    let ping = Ping {
        sender: kp_b.identity(),
        sender_addr: make_node_addr(&client_ep.local_addr().unwrap().to_string()),
    };
    let ping_body = to_cbor(&ping).unwrap();
    let ping_frame = Frame::new(MSG_PING, ping_body);

    let pong_response = mesh_transport::send_request(&conn_to_a, &ping_frame)
        .await
        .unwrap();
    assert_eq!(pong_response.msg_type, MSG_PONG);
    let pong: Pong = from_cbor(&pong_response.body).unwrap();
    assert_eq!(pong.sender, kp_a.identity());

    // ── Cleanup ──

    conn_to_a.close("test done");
    client_ep.close();
    handle_a.abort();
    handle_b.abort();
}

#[tokio::test]
async fn ping_pong_over_quic() {
    let kp_secret = Keypair::generate().secret_bytes();
    let kp = Keypair::from_bytes(&kp_secret);
    let ep = make_endpoint(&Keypair::from_bytes(&kp_secret));
    let addr = ep.local_addr().unwrap();
    let (_dht, handle) = start_node(Keypair::from_bytes(&kp_secret), ep);

    tokio::time::sleep(Duration::from_millis(50)).await;

    let client_ep = make_endpoint(&Keypair::generate());
    let conn = client_ep.connect(addr).await.unwrap();

    let ping = Ping {
        sender: Keypair::generate().identity(),
        sender_addr: make_node_addr("127.0.0.1:9999"),
    };
    let body = to_cbor(&ping).unwrap();
    let frame = Frame::new(MSG_PING, body);

    let resp = mesh_transport::send_request(&conn, &frame).await.unwrap();
    assert_eq!(resp.msg_type, MSG_PONG);

    let pong: Pong = from_cbor(&resp.body).unwrap();
    assert_eq!(pong.sender, kp.identity());
    assert_eq!(pong.observed_addr.address, "127.0.0.1:9999");

    conn.close("done");
    client_ep.close();
    handle.abort();
}

#[tokio::test]
async fn store_and_find_value_over_quic() {
    let kp_server_secret = Keypair::generate().secret_bytes();
    let _kp_server = Keypair::from_bytes(&kp_server_secret);
    let ep = make_endpoint(&Keypair::from_bytes(&kp_server_secret));
    let addr = ep.local_addr().unwrap();
    let (_dht, handle) = start_node(Keypair::from_bytes(&kp_server_secret), ep);

    tokio::time::sleep(Duration::from_millis(50)).await;

    let client_kp = Keypair::generate();
    let client_ep = make_endpoint(&client_kp);
    let conn = client_ep.connect(addr).await.unwrap();

    // Store a descriptor
    let publisher_kp = Keypair::generate();
    let cap_type = "storage/object/s3";
    let rk = routing_key(cap_type);

    let desc = Descriptor::create(
        &publisher_kp,
        schema_hash("core/capability"),
        cap_type.to_string(),
        b"s3-capability".to_vec(),
        now_micros(),
        1,
        3600,
        vec![rk.clone()],
    )
    .unwrap();

    let store = Store {
        sender: publisher_kp.identity(),
        sender_addr: make_node_addr(&client_ep.local_addr().unwrap().to_string()),
        descriptor: desc.clone(),
    };
    let store_body = to_cbor(&store).unwrap();
    let store_frame = Frame::new(MSG_STORE, store_body);

    let store_resp = mesh_transport::send_request(&conn, &store_frame)
        .await
        .unwrap();
    assert_eq!(store_resp.msg_type, MSG_STORE_ACK);
    let ack: StoreAck = from_cbor(&store_resp.body).unwrap();
    assert!(ack.stored);

    // Find it
    let find = FindValue {
        sender: client_kp.identity(),
        sender_addr: make_node_addr(&client_ep.local_addr().unwrap().to_string()),
        key: rk,
        max_results: 20,
        filters: None,
    };
    let find_body = to_cbor(&find).unwrap();
    let find_frame = Frame::new(MSG_FIND_VALUE, find_body);

    let find_resp = mesh_transport::send_request(&conn, &find_frame)
        .await
        .unwrap();
    assert_eq!(find_resp.msg_type, MSG_FIND_VALUE_RESULT);

    let result: FindValueResult = from_cbor(&find_resp.body).unwrap();
    let descs = result.descriptors.unwrap();
    assert_eq!(descs.len(), 1);
    assert_eq!(descs[0].id, desc.id);

    conn.close("done");
    client_ep.close();
    handle.abort();
}
