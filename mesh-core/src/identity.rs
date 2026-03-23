//! Identity and keypair types for mesh protocol (Section 1.3).
//!
//! Identities are public keys tagged with their algorithm. DIDs are
//! derived deterministically: `did:mesh:<base58btc(algo_byte || pubkey)>`.

use ed25519_dalek::{Signer, Verifier};
use serde::{Deserialize, Serialize};

use crate::error::{MeshError, Result};
use crate::hash::Hash;

/// Algorithm ID for Ed25519 signatures.
pub const ALG_ED25519: u8 = 0x01;

/// Algorithm ID for ML-DSA-65 (reserved, post-quantum).
pub const ALG_ML_DSA_65: u8 = 0x02;

/// Algorithm ID for X25519 key exchange.
pub const ALG_X25519: u8 = 0x05;

/// A public identity: algorithm tag + public key bytes (Section 1.3).
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Identity {
    /// Signature algorithm from the Algorithm Registry.
    pub algorithm: u8,
    /// Raw public key bytes.
    pub public_key: Vec<u8>,
}

impl Identity {
    /// Create a new identity from raw components.
    pub fn new(algorithm: u8, public_key: Vec<u8>) -> Self {
        Self {
            algorithm,
            public_key,
        }
    }

    /// Derive the DID for this identity: `did:mesh:<base58btc(algo || pubkey)>`.
    pub fn did(&self) -> String {
        let mut bytes = Vec::with_capacity(1 + self.public_key.len());
        bytes.push(self.algorithm);
        bytes.extend_from_slice(&self.public_key);
        format!("did:mesh:{}", bs58::encode(&bytes).into_string())
    }

    /// Verify a signature over the given message using this identity's public key.
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<()> {
        match self.algorithm {
            ALG_ED25519 => {
                let pubkey = ed25519_dalek::VerifyingKey::from_bytes(
                    self.public_key
                        .as_slice()
                        .try_into()
                        .map_err(|_| MeshError::InvalidSignature)?,
                )
                .map_err(|_| MeshError::InvalidSignature)?;
                let sig = ed25519_dalek::Signature::from_bytes(
                    signature
                        .try_into()
                        .map_err(|_| MeshError::InvalidSignature)?,
                );
                pubkey
                    .verify(message, &sig)
                    .map_err(|_| MeshError::InvalidSignature)
            }
            alg => Err(MeshError::UnknownAlgorithm(alg)),
        }
    }

    /// Compute the DHT node ID for this identity: `BLAKE3(public_key)` (Section 4.2).
    pub fn node_id(&self) -> Hash {
        Hash::blake3(&self.public_key)
    }
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Identity(0x{:02x}, {}...)",
            self.algorithm,
            &hex::encode(&self.public_key)[..std::cmp::min(16, self.public_key.len() * 2)]
        )
    }
}

/// An Ed25519 keypair for signing operations.
pub struct Keypair {
    signing_key: ed25519_dalek::SigningKey,
}

impl Keypair {
    /// Generate a new random Ed25519 keypair.
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rng);
        Self { signing_key }
    }

    /// Create a keypair from a 32-byte secret key.
    pub fn from_bytes(secret: &[u8; 32]) -> Self {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(secret);
        Self { signing_key }
    }

    /// Get the public identity for this keypair.
    pub fn identity(&self) -> Identity {
        Identity {
            algorithm: ALG_ED25519,
            public_key: self.signing_key.verifying_key().to_bytes().to_vec(),
        }
    }

    /// Sign a message, returning the signature bytes.
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        let sig = self.signing_key.sign(message);
        sig.to_bytes().to_vec()
    }

    /// Get the raw secret key bytes.
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_generate_and_identity() {
        let kp = Keypair::generate();
        let id = kp.identity();
        assert_eq!(id.algorithm, ALG_ED25519);
        assert_eq!(id.public_key.len(), 32);
    }

    #[test]
    fn did_derivation() {
        let kp = Keypair::generate();
        let id = kp.identity();
        let did = id.did();
        assert!(did.starts_with("did:mesh:"));
        // DID should be deterministic
        assert_eq!(did, id.did());
    }

    #[test]
    fn sign_and_verify() {
        let kp = Keypair::generate();
        let id = kp.identity();
        let message = b"hello mesh protocol";
        let sig = kp.sign(message);
        assert!(id.verify(message, &sig).is_ok());
    }

    #[test]
    fn verify_wrong_message() {
        let kp = Keypair::generate();
        let id = kp.identity();
        let sig = kp.sign(b"hello");
        assert!(id.verify(b"wrong", &sig).is_err());
    }

    #[test]
    fn verify_wrong_key() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();
        let sig = kp1.sign(b"hello");
        assert!(kp2.identity().verify(b"hello", &sig).is_err());
    }

    #[test]
    fn keypair_from_bytes_deterministic() {
        let kp = Keypair::generate();
        let secret = kp.secret_bytes();
        let kp2 = Keypair::from_bytes(&secret);
        assert_eq!(kp.identity(), kp2.identity());
    }

    #[test]
    fn node_id_deterministic() {
        let kp = Keypair::generate();
        let id = kp.identity();
        let n1 = id.node_id();
        let n2 = id.node_id();
        assert_eq!(n1, n2);
    }

    #[test]
    fn did_roundtrip_decode() {
        let kp = Keypair::generate();
        let id = kp.identity();
        let did = id.did();
        // Extract the base58 part
        let encoded = did.strip_prefix("did:mesh:").unwrap();
        let decoded = bs58::decode(encoded).into_vec().unwrap();
        assert_eq!(decoded[0], ALG_ED25519);
        assert_eq!(&decoded[1..], &id.public_key);
    }

    #[test]
    fn verify_unknown_algorithm() {
        let id = Identity::new(0x99, vec![0u8; 32]);
        let result = id.verify(b"message", &[0u8; 64]);
        assert!(matches!(result, Err(MeshError::UnknownAlgorithm(0x99))));
    }

    #[test]
    fn verify_wrong_key_length() {
        // Ed25519 key that's the wrong length
        let id = Identity::new(ALG_ED25519, vec![0u8; 16]); // too short
        let result = id.verify(b"message", &[0u8; 64]);
        assert!(matches!(result, Err(MeshError::InvalidSignature)));
    }

    #[test]
    fn did_format_correct() {
        let kp = Keypair::generate();
        let id = kp.identity();
        let did = id.did();
        // Must start with did:mesh:
        assert!(did.starts_with("did:mesh:"));
        // Decode and verify format: algo_byte || pubkey_bytes
        let encoded = did.strip_prefix("did:mesh:").unwrap();
        let decoded = bs58::decode(encoded).into_vec().unwrap();
        assert_eq!(decoded.len(), 33); // 1 byte algo + 32 bytes pubkey
        assert_eq!(decoded[0], ALG_ED25519);
    }
}
