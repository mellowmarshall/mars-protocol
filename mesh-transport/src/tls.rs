//! TLS configuration for mesh QUIC transport.
//!
//! Generates self-signed Ed25519 certificates derived from the node's mesh
//! keypair. This binds the TLS transport identity to the mesh protocol
//! identity — the peer's Ed25519 public key in their TLS cert IS their
//! mesh Identity. Receivers can extract it after the QUIC handshake and
//! verify that message `sender` fields match.
//!
//! Mutual TLS is enabled: both client and server present certificates and
//! verify the peer's. After the handshake, each side can extract the peer's
//! mesh [`Identity`] from their TLS certificate via [`identity_from_cert_der`].

use std::sync::Arc;

use mesh_core::identity::{ALG_ED25519, Identity, Keypair};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::danger::ClientCertVerified;
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};

use crate::error::{Result, TransportError};

/// ALPN protocol identifier for the mesh protocol (Section 8.1).
pub const MESH_ALPN: &[u8] = b"mesh/0";

/// Supported TLS signature verification schemes for mesh transport.
///
/// Ed25519 is preferred; other schemes are included for interoperability
/// with TLS libraries that may use different handshake signature schemes.
fn supported_schemes() -> Vec<SignatureScheme> {
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

/// Server certificate verifier for mesh transport (client-side).
///
/// Mesh nodes use self-signed Ed25519 certificates — there is no CA chain
/// to verify. The TLS handshake itself proves the peer holds the private
/// key corresponding to the certificate's public key. After the handshake,
/// the peer's Ed25519 public key is extracted from the certificate and used
/// as their mesh [`Identity`].
///
/// # Security model
///
/// The TLS handshake proves key possession. The extracted identity is then
/// bound to protocol-level messages via sender-TLS binding (Task 1c).
///
/// TODO: Verify the self-signed certificate's signature (i.e., that the
/// cert's signature over its TBS data is valid using the embedded public
/// key). This would reject malformed/forged certs at the TLS layer rather
/// than relying solely on the handshake proof-of-possession.
#[derive(Debug)]
struct MeshServerCertVerifier;

impl ServerCertVerifier for MeshServerCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, RustlsError> {
        // Verify the certificate contains an Ed25519 public key.
        // We don't verify the self-signed signature here (the TLS handshake
        // proves key possession), but we reject certs without an extractable
        // Ed25519 key since they can't be bound to a mesh identity.
        if extract_ed25519_pubkey_from_cert_der(end_entity.as_ref()).is_none() {
            return Err(RustlsError::InvalidCertificate(
                rustls::CertificateError::BadEncoding,
            ));
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        supported_schemes()
    }
}

/// Client certificate verifier for mesh transport (server-side).
///
/// Enables mutual TLS so the server can extract the client's mesh
/// [`Identity`] from their TLS certificate. Uses the same permissive
/// verification model as [`MeshServerCertVerifier`] — we accept any
/// self-signed cert with an extractable Ed25519 key.
///
/// Client auth is mandatory: every mesh peer MUST present a certificate
/// so its identity can be verified via sender-TLS binding.
#[derive(Debug)]
struct MeshClientCertVerifier;

impl rustls::server::danger::ClientCertVerifier for MeshClientCertVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        // No CA hints — mesh nodes use self-signed certs, not a PKI.
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> std::result::Result<ClientCertVerified, RustlsError> {
        // Ensure we can extract an Ed25519 key, which proves this is a
        // valid mesh identity cert.
        if extract_ed25519_pubkey_from_cert_der(end_entity.as_ref()).is_none() {
            return Err(RustlsError::InvalidCertificate(
                rustls::CertificateError::BadEncoding,
            ));
        }
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, RustlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        supported_schemes()
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

/// Build a rustls [`ServerConfig`] for mesh transport with mutual TLS.
///
/// The server presents its own certificate and requests (but does not require)
/// client certificates. This enables the server to extract the client's mesh
/// [`Identity`] from their TLS cert after the handshake.
pub fn server_crypto_config(
    cert_chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<rustls::ServerConfig>> {
    let mut config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(Arc::new(MeshClientCertVerifier))
        .with_single_cert(cert_chain, key)
        .map_err(|e| TransportError::Tls(format!("server config: {e}")))?;

    config.alpn_protocols = vec![MESH_ALPN.to_vec()];
    Ok(Arc::new(config))
}

/// Build a rustls [`ClientConfig`] for mesh transport with mutual TLS.
///
/// Uses [`MeshServerCertVerifier`] which accepts self-signed certs (no PKI).
/// The client presents its own certificate so the server can extract the
/// client's mesh [`Identity`] from the TLS handshake.
pub fn client_crypto_config(
    cert_chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<rustls::ClientConfig>> {
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(MeshServerCertVerifier))
        .with_client_auth_cert(cert_chain, key)
        .map_err(|e| TransportError::Tls(format!("client config: {e}")))?;

    config.alpn_protocols = vec![MESH_ALPN.to_vec()];
    Ok(Arc::new(config))
}
