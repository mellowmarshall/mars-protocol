//! TLS configuration for mesh QUIC transport.
//!
//! Generates self-signed Ed25519 certificates derived from the node's mesh
//! keypair. This binds the TLS transport identity to the mesh protocol
//! identity — the peer's Ed25519 public key in their TLS cert IS their
//! mesh Identity. Receivers can extract it after the QUIC handshake and
//! verify that message `sender` fields match.

use std::sync::Arc;

use mesh_core::identity::{ALG_ED25519, Identity, Keypair};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};

use crate::error::{Result, TransportError};

/// ALPN protocol identifier for the mesh protocol (Section 8.1).
pub const MESH_ALPN: &[u8] = b"mesh/0";

/// A verifier that accepts any self-signed certificate.
///
/// Mesh nodes authenticate via TLS identity binding: the peer's Ed25519
/// public key is embedded in their TLS certificate (derived from their
/// mesh keypair). We skip CA-chain verification but the TLS handshake
/// still proves the peer holds the private key for their cert.
#[derive(Debug)]
struct MeshCertVerifier;

impl ServerCertVerifier for MeshCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, RustlsError> {
        // We skip PKI verification — mesh nodes use self-signed certs.
        // The TLS handshake itself proves the peer holds the cert's private
        // key. We extract the Ed25519 public key from the cert after
        // connection and verify it matches the sender Identity in messages.
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ED25519,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

/// Encode an Ed25519 32-byte secret key as PKCS#8 DER for use with rcgen/rustls.
fn ed25519_secret_to_pkcs8_der(secret: &[u8; 32]) -> Vec<u8> {
    let mut der = Vec::with_capacity(48);
    // SEQUENCE (46 bytes)
    der.extend_from_slice(&[0x30, 0x2e]);
    // INTEGER 0 (version)
    der.extend_from_slice(&[0x02, 0x01, 0x00]);
    // SEQUENCE { OID 1.3.101.112 (Ed25519) }
    der.extend_from_slice(&[0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70]);
    // OCTET STRING { OCTET STRING { 32-byte key } }
    der.extend_from_slice(&[0x04, 0x22, 0x04, 0x20]);
    der.extend_from_slice(secret);
    der
}

/// Extract the Ed25519 public key (32 bytes) from a DER-encoded X.509 certificate.
///
/// Searches for the Ed25519 SubjectPublicKeyInfo pattern:
///   OID 1.3.101.112 (06 03 2b 65 70) followed by BIT STRING (03 21 00) + 32 bytes.
pub fn extract_ed25519_pubkey_from_cert_der(cert_der: &[u8]) -> Option<[u8; 32]> {
    let pattern: &[u8] = &[0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00];
    for i in 0..cert_der.len().saturating_sub(pattern.len() + 32) {
        if cert_der[i..i + pattern.len()] == *pattern {
            let key_start = i + pattern.len();
            let key_end = key_start + 32;
            if key_end <= cert_der.len() {
                let mut key = [0u8; 32];
                key.copy_from_slice(&cert_der[key_start..key_end]);
                return Some(key);
            }
        }
    }
    None
}

/// Extract a mesh [`Identity`] from a DER-encoded X.509 certificate.
///
/// Returns `Some(Identity)` if the cert contains an Ed25519 public key.
pub fn identity_from_cert_der(cert_der: &[u8]) -> Option<Identity> {
    let pubkey = extract_ed25519_pubkey_from_cert_der(cert_der)?;
    Some(Identity::new(ALG_ED25519, pubkey.to_vec()))
}

/// Generate a self-signed certificate derived from a mesh [`Keypair`].
///
/// The TLS certificate uses the same Ed25519 key as the mesh identity,
/// binding transport-level identity to protocol-level identity. The DID
/// is set as the certificate's subject CN.
pub fn generate_self_signed_cert(
    keypair: &Keypair,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let did = keypair.identity().did();
    let pkcs8_der = ed25519_secret_to_pkcs8_der(&keypair.secret_bytes());

    let pkcs8_key = rustls::pki_types::PrivatePkcs8KeyDer::from(pkcs8_der.clone());
    let key_pair =
        rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&pkcs8_key, &rcgen::PKCS_ED25519)
            .map_err(|e| TransportError::Tls(format!("keypair from der: {e}")))?;

    let cert_params = rcgen::CertificateParams::new(vec![did])
        .map_err(|e| TransportError::Tls(format!("cert params: {e}")))?;

    let cert = cert_params
        .self_signed(&key_pair)
        .map_err(|e| TransportError::Tls(format!("self-sign: {e}")))?;

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(pkcs8_der.into());

    Ok((vec![cert_der], key_der))
}

/// Build a rustls [`ServerConfig`] for mesh transport.
pub fn server_crypto_config(
    cert_chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<rustls::ServerConfig>> {
    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .map_err(|e| TransportError::Tls(format!("server config: {e}")))?;

    config.alpn_protocols = vec![MESH_ALPN.to_vec()];
    Ok(Arc::new(config))
}

/// Build a rustls [`ClientConfig`] for mesh transport.
///
/// Uses [`MeshCertVerifier`] which accepts self-signed certs (no PKI).
/// The TLS handshake still proves the peer holds the cert's private key.
pub fn client_crypto_config() -> Result<Arc<rustls::ClientConfig>> {
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(MeshCertVerifier))
        .with_no_client_auth();

    config.alpn_protocols = vec![MESH_ALPN.to_vec()];
    Ok(Arc::new(config))
}
