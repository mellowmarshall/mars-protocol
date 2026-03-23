//! DID-Auth challenge-response module for identity verification.

use std::time::{SystemTime, UNIX_EPOCH};

use mesh_core::identity::Identity;
use serde::Serialize;
use uuid::Uuid;

/// Challenge validity window: 5 minutes in microseconds.
const CHALLENGE_TTL_MICROS: u64 = 5 * 60 * 1_000_000;

/// A DID-Auth challenge issued by the hub.
#[derive(Debug, Clone, Serialize)]
pub struct DIDAuthChallenge {
    pub id: Uuid,
    #[serde(serialize_with = "hex_bytes")]
    pub nonce: [u8; 32],
    pub hub_did: String,
    pub action: String,
    pub issued_at: u64,
    pub expiry: u64,
}

fn hex_bytes<S: serde::Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&hex::encode(bytes))
}

impl DIDAuthChallenge {
    /// Create a new challenge with a random nonce and 5-minute expiry.
    pub fn new(hub_did: &str, action: &str) -> Self {
        use rand::RngCore;
        let mut nonce = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut nonce);
        let issued_at = now_micros();
        let expiry = issued_at + CHALLENGE_TTL_MICROS;

        Self {
            id: Uuid::new_v4(),
            nonce,
            hub_did: hub_did.to_string(),
            action: action.to_string(),
            issued_at,
            expiry,
        }
    }

    /// Produce the canonical signable bytes for this challenge.
    ///
    /// Uses a CBOR map with keys in RFC 8949 deterministic order:
    /// action(6), nonce(5), hub_did(7), issued_at(9)
    /// Sorted by encoded key length then lexicographic.
    pub fn to_signable_bytes(&self) -> Vec<u8> {
        use ciborium::Value;

        // RFC 8949 §4.2.1: sort by encoded key bytes.
        // For text strings: shorter keys first, then lexicographic.
        // "nonce" (5) < "action" (6) < "hub_did" (7) < "issued_at" (9)
        let map = Value::Map(vec![
            (
                Value::Text("nonce".to_string()),
                Value::Bytes(self.nonce.to_vec()),
            ),
            (
                Value::Text("action".to_string()),
                Value::Text(self.action.clone()),
            ),
            (
                Value::Text("hub_did".to_string()),
                Value::Text(self.hub_did.clone()),
            ),
            (
                Value::Text("issued_at".to_string()),
                Value::Integer(self.issued_at.into()),
            ),
        ]);

        let mut buf = Vec::new();
        ciborium::into_writer(&map, &mut buf).expect("CBOR serialization should not fail");
        buf
    }

    /// Verify a signature over the signable bytes using the given identity.
    pub fn verify(&self, identity: &Identity, signature: &[u8]) -> Result<(), String> {
        let signable = self.to_signable_bytes();
        identity
            .verify(&signable, signature)
            .map_err(|e| format!("signature verification failed: {e}"))
    }

    /// Check if this challenge has expired.
    pub fn is_expired(&self, now_micros: u64) -> bool {
        now_micros >= self.expiry
    }
}

fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::identity::Keypair;

    #[test]
    fn challenge_create_and_signable() {
        let challenge = DIDAuthChallenge::new("did:mesh:zHubDid", "register_identity");
        assert_eq!(challenge.action, "register_identity");
        assert_eq!(challenge.hub_did, "did:mesh:zHubDid");
        assert!(challenge.expiry > challenge.issued_at);

        // Signable bytes should be deterministic for same challenge
        let bytes1 = challenge.to_signable_bytes();
        let bytes2 = challenge.to_signable_bytes();
        assert_eq!(bytes1, bytes2);
        assert!(!bytes1.is_empty());
    }

    #[test]
    fn challenge_verify_valid_signature() {
        let kp = Keypair::generate();
        let identity = kp.identity();
        let challenge = DIDAuthChallenge::new(&identity.did(), "register_identity");

        let signable = challenge.to_signable_bytes();
        let signature = kp.sign(&signable);

        assert!(challenge.verify(&identity, &signature).is_ok());
    }

    #[test]
    fn challenge_verify_wrong_signature() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();
        let challenge = DIDAuthChallenge::new(&kp1.identity().did(), "register_identity");

        let signable = challenge.to_signable_bytes();
        let signature = kp2.sign(&signable); // signed by wrong key

        assert!(challenge.verify(&kp1.identity(), &signature).is_err());
    }

    #[test]
    fn challenge_expiry() {
        let challenge = DIDAuthChallenge::new("did:mesh:zHubDid", "register_identity");

        // Not expired at issued time
        assert!(!challenge.is_expired(challenge.issued_at));
        // Not expired 1 second before expiry
        assert!(!challenge.is_expired(challenge.expiry - 1));
        // Expired at expiry time
        assert!(challenge.is_expired(challenge.expiry));
        // Expired well after
        assert!(challenge.is_expired(challenge.expiry + 1_000_000));
    }

    #[test]
    fn challenge_consume_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let tm = crate::tenant::TenantManager::open(&dir.path().join("tenants.db")).unwrap();

        let tenant = tm.create_tenant("test-org", "free").unwrap();
        let challenge = tm
            .create_challenge(&tenant.id, "did:mesh:zHubDid", "register_identity")
            .unwrap();

        // Challenge should be retrievable
        let loaded = tm.get_challenge(&challenge.id).unwrap().unwrap();
        assert_eq!(loaded.id, challenge.id);
        assert_eq!(loaded.action, "register_identity");

        // First consume should succeed
        assert!(tm.consume_challenge(&challenge.id).is_ok());

        // Second consume should fail (already consumed)
        assert!(tm.consume_challenge(&challenge.id).is_err());
    }
}
