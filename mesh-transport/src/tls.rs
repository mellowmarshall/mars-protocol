//! TLS configuration for mesh QUIC transport.
//!
//! Generates self-signed certificates and configures rustls for mesh use.
//! Authentication happens at the protocol layer (descriptor signatures),
//! not at the TLS layer, so we skip server certificate verification.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};

use crate::error::{Result, TransportError};

/// ALPN protocol identifier for the mesh protocol (Section 8.1).
pub const MESH_ALPN: &[u8] = b"mesh/0";

/// A verifier that accepts any server certificate.
///
/// Mesh nodes authenticate via protocol-level signatures (DID identity),
/// not TLS certificates. The TLS layer provides only transport encryption.
#[derive(Debug)]
struct SkipServerVerification;

impl ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, RustlsError> {
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
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

/// Generate a self-signed certificate and private key for mesh transport.
pub fn generate_self_signed_cert() -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>
{
    let cert_params = rcgen::CertificateParams::new(vec!["mesh.local".to_string()])
        .map_err(|e| TransportError::Tls(format!("cert params: {e}")))?;
    let key_pair = rcgen::KeyPair::generate()
        .map_err(|e| TransportError::Tls(format!("keypair generation: {e}")))?;
    let cert = cert_params
        .self_signed(&key_pair)
        .map_err(|e| TransportError::Tls(format!("self-sign: {e}")))?;

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(key_pair.serialize_der().into());

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
/// Skips server certificate verification since mesh authentication
/// is handled at the protocol layer.
pub fn client_crypto_config() -> Result<Arc<rustls::ClientConfig>> {
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();

    config.alpn_protocols = vec![MESH_ALPN.to_vec()];
    Ok(Arc::new(config))
}
