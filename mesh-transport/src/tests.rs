//! Integration tests for mesh-transport.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use mesh_core::Frame;
use mesh_core::frame::{MSG_PING, MSG_PONG, MSG_STORE, MSG_STORE_ACK};
use mesh_core::identity::Keypair;

use crate::connection::{recv_frame, send_frame, send_request};
use crate::endpoint::MeshEndpoint;
use crate::tls;

/// Helper: create a localhost address with OS-assigned port.
fn localhost() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
}

/// Helper: create an endpoint with a fresh keypair.
fn make_endpoint() -> MeshEndpoint {
    let kp = Keypair::generate();
    MeshEndpoint::new(localhost(), &kp).unwrap()
}

#[test]
fn tls_self_signed_cert_generation() {
    let kp = Keypair::generate();
    let (certs, _key) = tls::generate_self_signed_cert(&kp).unwrap();
    assert_eq!(certs.len(), 1);
    assert!(!certs[0].is_empty());

    // Verify the cert contains the same Ed25519 public key
    let extracted = tls::extract_ed25519_pubkey_from_cert_der(certs[0].as_ref());
    assert!(extracted.is_some());
    let identity = tls::identity_from_cert_der(certs[0].as_ref()).unwrap();
    assert_eq!(identity, kp.identity());
}

#[test]
fn tls_server_crypto_config() {
    let kp = Keypair::generate();
    let (certs, key) = tls::generate_self_signed_cert(&kp).unwrap();
    let config = tls::server_crypto_config(certs, key).unwrap();
    assert_eq!(config.alpn_protocols, vec![tls::MESH_ALPN.to_vec()]);
}

#[test]
fn tls_client_crypto_config() {
    let config = tls::client_crypto_config().unwrap();
    assert_eq!(config.alpn_protocols, vec![tls::MESH_ALPN.to_vec()]);
}

#[tokio::test]
async fn endpoint_creation() {
    let ep = make_endpoint();
    let addr = ep.local_addr().unwrap();
    assert!(addr.port() > 0);
    ep.close();
}

#[tokio::test]
async fn connect_and_ping_pong() {
    let server_ep = make_endpoint();
    let server_addr = server_ep.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        server_ep
            .listen(|frame, sender, _peer_identity| async move {
                assert_eq!(frame.msg_type, MSG_PING);
                let response = Frame::response(&frame, MSG_PONG, b"pong-body".to_vec());
                sender.send(&response).await.unwrap();
            })
            .await
            .unwrap();
    });

    let client_ep = make_endpoint();
    let conn = client_ep.connect(server_addr).await.unwrap();

    let ping = Frame::new(MSG_PING, b"ping-body".to_vec());
    let pong = send_request(&conn, &ping).await.unwrap();

    assert_eq!(pong.msg_type, MSG_PONG);
    assert_eq!(pong.msg_id, ping.msg_id);
    assert_eq!(pong.body, b"pong-body");

    conn.close("test done");
    client_ep.close();
    server_handle.abort();
}

#[tokio::test]
async fn frame_roundtrip_over_quic() {
    let server_ep = make_endpoint();
    let server_addr = server_ep.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        server_ep
            .listen(|frame, sender, _peer_identity| async move {
                let response = Frame::response(&frame, frame.msg_type | 0x80, frame.body.clone());
                sender.send(&response).await.unwrap();
            })
            .await
            .unwrap();
    });

    let client_ep = make_endpoint();
    let conn = client_ep.connect(server_addr).await.unwrap();

    // Test with various body sizes.
    for size in [0, 1, 100, 10_000, 65_536] {
        let body = vec![0xAB; size];
        let request = Frame::new(MSG_STORE, body.clone());
        let response = send_request(&conn, &request).await.unwrap();
        assert_eq!(response.msg_type, MSG_STORE_ACK);
        assert_eq!(response.body, body);
        assert_eq!(response.msg_id, request.msg_id);
    }

    conn.close("test done");
    client_ep.close();
    server_handle.abort();
}

#[tokio::test]
async fn multiple_concurrent_streams() {
    let server_ep = make_endpoint();
    let server_addr = server_ep.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        server_ep
            .listen(|frame, sender, _peer_identity| async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                let response = Frame::response(&frame, MSG_PONG, frame.body.clone());
                sender.send(&response).await.unwrap();
            })
            .await
            .unwrap();
    });

    let client_ep = make_endpoint();
    let conn = client_ep.connect(server_addr).await.unwrap();

    let mut handles = Vec::new();
    for i in 0u8..10 {
        let conn = conn.clone();
        handles.push(tokio::spawn(async move {
            let request = Frame::new(MSG_PING, vec![i]);
            let response = send_request(&conn, &request).await.unwrap();
            assert_eq!(response.msg_type, MSG_PONG);
            assert_eq!(response.body, vec![i]);
            assert_eq!(response.msg_id, request.msg_id);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    conn.close("test done");
    client_ep.close();
    server_handle.abort();
}

#[tokio::test]
async fn raw_stream_send_recv() {
    // Test the lower-level send_frame / recv_frame API directly.
    let server_ep = make_endpoint();
    let server_addr = server_ep.local_addr().unwrap();

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    let server_handle = tokio::spawn(async move {
        // Accept one connection, one stream, respond, then wait for signal.
        if let Some(incoming) = server_ep.inner().accept().await {
            let conn = incoming.await.unwrap();
            let mesh_conn = crate::MeshConnection::new(conn);
            let (mut send, mut recv) = mesh_conn.accept_stream().await.unwrap();
            let frame = recv_frame(&mut recv).await.unwrap();
            let response = Frame::response(&frame, MSG_PONG, b"raw-response".to_vec());
            send_frame(&mut send, &response).await.unwrap();
            // Wait for client to finish reading before closing.
            let _ = rx.await;
        }
    });

    let client_ep = make_endpoint();
    let conn = client_ep.connect(server_addr).await.unwrap();
    let (mut send, mut recv) = conn.open_stream().await.unwrap();

    let request = Frame::new(MSG_PING, b"raw-request".to_vec());
    send_frame(&mut send, &request).await.unwrap();
    let response = recv_frame(&mut recv).await.unwrap();

    assert_eq!(response.msg_type, MSG_PONG);
    assert_eq!(response.body, b"raw-response");

    // Signal server it's ok to close.
    let _ = tx.send(());
    conn.close("done");
    client_ep.close();
    server_handle.abort();
}

#[tokio::test]
async fn connection_reuse() {
    let server_ep = make_endpoint();
    let server_addr = server_ep.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        server_ep
            .listen(|frame, sender, _peer_identity| async move {
                let response = Frame::response(&frame, MSG_PONG, frame.body.clone());
                sender.send(&response).await.unwrap();
            })
            .await
            .unwrap();
    });

    let client_ep = make_endpoint();
    let conn = client_ep.connect(server_addr).await.unwrap();

    // Send 20 sequential requests on the same connection.
    for i in 0u16..20 {
        let request = Frame::new(MSG_PING, i.to_be_bytes().to_vec());
        let response = send_request(&conn, &request).await.unwrap();
        assert_eq!(response.msg_type, MSG_PONG);
        assert_eq!(response.body, i.to_be_bytes());
    }

    conn.close("test done");
    client_ep.close();
    server_handle.abort();
}

#[tokio::test]
async fn request_response_api() {
    // Test the accept_request / ResponseSender API explicitly.
    let server_ep = make_endpoint();
    let server_addr = server_ep.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        server_ep
            .listen(|frame, sender, _peer_identity| async move {
                // Verify it's a well-formed frame.
                assert_eq!(frame.magic, mesh_core::frame::FRAME_MAGIC);
                assert_eq!(frame.version, mesh_core::frame::PROTOCOL_VERSION);
                assert!(!frame.is_response());

                let mut response_body = frame.body.clone();
                response_body.extend_from_slice(b"-processed");
                let response = Frame::response(&frame, frame.msg_type | 0x80, response_body);
                sender.send(&response).await.unwrap();
            })
            .await
            .unwrap();
    });

    let client_ep = make_endpoint();
    let conn = client_ep.connect(server_addr).await.unwrap();

    let request = Frame::new(MSG_STORE, b"test-data".to_vec());
    let response = send_request(&conn, &request).await.unwrap();

    assert_eq!(response.msg_type, MSG_STORE_ACK);
    assert!(response.is_response());
    assert_eq!(response.body, b"test-data-processed");

    conn.close("done");
    client_ep.close();
    server_handle.abort();
}
