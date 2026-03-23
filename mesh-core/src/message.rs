//! Protocol messages — CBOR-serializable structs for all 8 message types
//! (Sections 3.4–3.7).

use serde::{Deserialize, Serialize};

use crate::descriptor::Descriptor;
use crate::hash::Hash;
use crate::identity::Identity;

/// A node's network address.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeAddr {
    /// Protocol (e.g., "quic").
    pub protocol: String,
    /// Address string (e.g., "198.51.100.42:4433").
    pub address: String,
}

/// Information about a known node (used in FIND_NODE_RESULT, FIND_VALUE_RESULT).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    /// The node's public identity.
    pub identity: Identity,
    /// The node's network address.
    pub addr: NodeAddr,
    /// Timestamp of last successful contact (microseconds since epoch).
    pub last_seen: u64,
}

/// Optional filters for FIND_VALUE requests (Section 3.7.1).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FilterSet {
    /// Only return descriptors with this schema hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_hash: Option<Hash>,
    /// Only return descriptors newer than this timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_timestamp: Option<u64>,
    /// Only return descriptors from this publisher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Identity>,
}

// ── Request messages ──

/// PING message (0x01) — liveness check (Section 3.4).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ping {
    /// Sender's identity.
    pub sender: Identity,
    /// Sender's QUIC endpoint.
    pub sender_addr: NodeAddr,
}

/// STORE message (0x02) — store a descriptor (Section 3.5).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Store {
    /// Sender's identity.
    pub sender: Identity,
    /// The descriptor to store.
    pub descriptor: Descriptor,
}

/// FIND_NODE message (0x03) — find nodes closest to a key (Section 3.6).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FindNode {
    /// Sender's identity.
    pub sender: Identity,
    /// The DHT key to find nodes near.
    pub target: Hash,
}

/// FIND_VALUE message (0x04) — find descriptors at a key (Section 3.7).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FindValue {
    /// Sender's identity.
    pub sender: Identity,
    /// The routing key to search for.
    pub key: Hash,
    /// Max descriptors to return (default 20).
    pub max_results: u16,
    /// Optional payload-level filters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<FilterSet>,
}

// ── Response messages ──

/// PONG message (0x81) — liveness confirmation (Section 3.4).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pong {
    /// Responder's identity.
    pub sender: Identity,
    /// Responder's QUIC endpoint.
    pub sender_addr: NodeAddr,
    /// What the sender's address looks like from our side.
    pub observed_addr: NodeAddr,
}

/// STORE_ACK message (0x82) — acknowledge storage (Section 3.5).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoreAck {
    /// Whether the node accepted the descriptor.
    pub stored: bool,
    /// If not stored, why (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// FIND_NODE_RESULT message (0x83) — return closest nodes (Section 3.6).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FindNodeResult {
    /// Up to K closest nodes (K=20 default).
    pub nodes: Vec<NodeInfo>,
}

/// FIND_VALUE_RESULT message (0x84) — return descriptors or closer nodes (Section 3.7).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FindValueResult {
    /// Matching descriptors (if this node has them).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descriptors: Option<Vec<Descriptor>>,
    /// Closest nodes (if this node doesn't have matching descriptors).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<Vec<NodeInfo>>,
}

/// Helper: serialize a message to CBOR bytes.
pub fn to_cbor<T: Serialize>(msg: &T) -> crate::error::Result<Vec<u8>> {
    let mut buf = Vec::new();
    ciborium::into_writer(msg, &mut buf)
        .map_err(|e| crate::error::MeshError::Cbor(e.to_string()))?;
    Ok(buf)
}

/// Helper: deserialize a message from CBOR bytes.
pub fn from_cbor<T: for<'de> Deserialize<'de>>(data: &[u8]) -> crate::error::Result<T> {
    ciborium::from_reader(data).map_err(|e| crate::error::MeshError::Cbor(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Keypair;

    #[test]
    fn ping_cbor_roundtrip() {
        let kp = Keypair::generate();
        let ping = Ping {
            sender: kp.identity(),
            sender_addr: NodeAddr {
                protocol: "quic".into(),
                address: "127.0.0.1:4433".into(),
            },
        };
        let bytes = to_cbor(&ping).unwrap();
        let decoded: Ping = from_cbor(&bytes).unwrap();
        assert_eq!(decoded.sender, ping.sender);
        assert_eq!(decoded.sender_addr, ping.sender_addr);
    }

    #[test]
    fn pong_cbor_roundtrip() {
        let kp = Keypair::generate();
        let pong = Pong {
            sender: kp.identity(),
            sender_addr: NodeAddr {
                protocol: "quic".into(),
                address: "127.0.0.1:4433".into(),
            },
            observed_addr: NodeAddr {
                protocol: "quic".into(),
                address: "203.0.113.1:12345".into(),
            },
        };
        let bytes = to_cbor(&pong).unwrap();
        let decoded: Pong = from_cbor(&bytes).unwrap();
        assert_eq!(decoded.observed_addr, pong.observed_addr);
    }

    #[test]
    fn store_ack_roundtrip() {
        let ack = StoreAck {
            stored: true,
            reason: None,
        };
        let bytes = to_cbor(&ack).unwrap();
        let decoded: StoreAck = from_cbor(&bytes).unwrap();
        assert!(decoded.stored);
        assert!(decoded.reason.is_none());
    }

    #[test]
    fn store_ack_with_reason() {
        let ack = StoreAck {
            stored: false,
            reason: Some("capacity exceeded".into()),
        };
        let bytes = to_cbor(&ack).unwrap();
        let decoded: StoreAck = from_cbor(&bytes).unwrap();
        assert!(!decoded.stored);
        assert_eq!(decoded.reason.as_deref(), Some("capacity exceeded"));
    }

    #[test]
    fn find_node_roundtrip() {
        let kp = Keypair::generate();
        let msg = FindNode {
            sender: kp.identity(),
            target: Hash::blake3(b"target key"),
        };
        let bytes = to_cbor(&msg).unwrap();
        let decoded: FindNode = from_cbor(&bytes).unwrap();
        assert_eq!(decoded.target, msg.target);
    }

    #[test]
    fn find_value_with_filters() {
        let kp = Keypair::generate();
        let msg = FindValue {
            sender: kp.identity(),
            key: Hash::blake3(b"routing key"),
            max_results: 10,
            filters: Some(FilterSet {
                schema_hash: Some(Hash::blake3(b"schema")),
                min_timestamp: Some(1_000_000),
                publisher: None,
            }),
        };
        let bytes = to_cbor(&msg).unwrap();
        let decoded: FindValue = from_cbor(&bytes).unwrap();
        assert_eq!(decoded.max_results, 10);
        assert!(decoded.filters.unwrap().schema_hash.is_some());
    }

    #[test]
    fn find_node_result_roundtrip() {
        let kp = Keypair::generate();
        let result = FindNodeResult {
            nodes: vec![NodeInfo {
                identity: kp.identity(),
                addr: NodeAddr {
                    protocol: "quic".into(),
                    address: "10.0.0.1:4433".into(),
                },
                last_seen: 1_000_000,
            }],
        };
        let bytes = to_cbor(&result).unwrap();
        let decoded: FindNodeResult = from_cbor(&bytes).unwrap();
        assert_eq!(decoded.nodes.len(), 1);
    }

    #[test]
    fn find_value_result_with_descriptors() {
        let result = FindValueResult {
            descriptors: Some(vec![]),
            nodes: None,
        };
        let bytes = to_cbor(&result).unwrap();
        let decoded: FindValueResult = from_cbor(&bytes).unwrap();
        assert!(decoded.descriptors.is_some());
        assert!(decoded.nodes.is_none());
    }

    #[test]
    fn find_value_result_with_nodes() {
        let result = FindValueResult {
            descriptors: None,
            nodes: Some(vec![]),
        };
        let bytes = to_cbor(&result).unwrap();
        let decoded: FindValueResult = from_cbor(&bytes).unwrap();
        assert!(decoded.descriptors.is_none());
        assert!(decoded.nodes.is_some());
    }

    #[test]
    fn malformed_cbor_rejected() {
        let garbage = vec![0xFF, 0xFE, 0xFD];
        let result = from_cbor::<Ping>(&garbage);
        assert!(result.is_err());
    }

    #[test]
    fn empty_cbor_rejected() {
        let result = from_cbor::<StoreAck>(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn find_value_default_filters() {
        let kp = Keypair::generate();
        let msg = FindValue {
            sender: kp.identity(),
            key: Hash::blake3(b"test"),
            max_results: 20,
            filters: None,
        };
        let bytes = to_cbor(&msg).unwrap();
        let decoded: FindValue = from_cbor(&bytes).unwrap();
        assert!(decoded.filters.is_none());
        assert_eq!(decoded.max_results, 20);
    }
}
