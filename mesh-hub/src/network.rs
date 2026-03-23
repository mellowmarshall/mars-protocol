//! Network address validation for SSRF prevention.
//!
//! When hubs make outbound connections (peer discovery, gossip, delegated routing),
//! they must not connect to private/loopback/RFC1918 addresses unless explicitly
//! allowed by the operator. This prevents SSRF attacks where a malicious descriptor
//! could trick the hub into connecting to internal services.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Validate that an address is safe for outbound connections.
///
/// Rejects private, loopback, link-local, and reserved addresses
/// unless they appear in the operator allowlist.
pub fn validate_outbound_addr(
    addr: &str,
    allowlist: &[String],
) -> Result<SocketAddr, AddressValidationError> {
    // 1. Parse the address string to SocketAddr
    let socket_addr: SocketAddr = addr
        .parse()
        .map_err(|e| AddressValidationError::ParseError(format!("invalid address '{}': {}", addr, e)))?;

    // 2. Check if the address is in the allowlist (if so, always allow)
    if allowlist.iter().any(|a| a == addr) {
        return Ok(socket_addr);
    }

    // 3. Check if the IP is private/loopback/reserved
    if is_private_ip(&socket_addr.ip()) {
        return Err(AddressValidationError::PrivateAddress(format!(
            "address '{}' is private/loopback/reserved and not in the outbound allowlist",
            addr
        )));
    }

    // 4. Return the parsed SocketAddr
    Ok(socket_addr)
}

/// Returns `true` if the IP address is private, loopback, link-local, or reserved.
fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || is_cgn(v4)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || is_ula(v6)
                || is_link_local_v6(v6)
        }
    }
}

/// Check for Carrier-Grade NAT range: 100.64.0.0/10.
fn is_cgn(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 100 && (octets[1] & 0xC0) == 64
}

/// Check for IPv6 Unique Local Address range: fc00::/7.
fn is_ula(ip: &Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

/// Check for IPv6 link-local range: fe80::/10.
fn is_link_local_v6(ip: &Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

/// Errors returned by [`validate_outbound_addr`].
#[derive(Debug)]
pub enum AddressValidationError {
    /// The address string could not be parsed as a `SocketAddr`.
    ParseError(String),
    /// The address is private/loopback/reserved and not in the allowlist.
    PrivateAddress(String),
}

impl fmt::Display for AddressValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddressValidationError::ParseError(msg) => write!(f, "{}", msg),
            AddressValidationError::PrivateAddress(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for AddressValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_addresses_allowed() {
        assert!(validate_outbound_addr("8.8.8.8:443", &[]).is_ok());
        assert!(validate_outbound_addr("1.1.1.1:443", &[]).is_ok());
    }

    #[test]
    fn test_loopback_rejected() {
        let result = validate_outbound_addr("127.0.0.1:4433", &[]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AddressValidationError::PrivateAddress(_)));
    }

    #[test]
    fn test_rfc1918_rejected() {
        let result_10 = validate_outbound_addr("10.0.0.1:4433", &[]);
        assert!(result_10.is_err());
        assert!(matches!(result_10.unwrap_err(), AddressValidationError::PrivateAddress(_)));

        let result_172 = validate_outbound_addr("172.16.0.1:4433", &[]);
        assert!(result_172.is_err());
        assert!(matches!(result_172.unwrap_err(), AddressValidationError::PrivateAddress(_)));

        let result_192 = validate_outbound_addr("192.168.1.1:4433", &[]);
        assert!(result_192.is_err());
        assert!(matches!(result_192.unwrap_err(), AddressValidationError::PrivateAddress(_)));
    }

    #[test]
    fn test_link_local_rejected() {
        let result = validate_outbound_addr("169.254.1.1:4433", &[]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AddressValidationError::PrivateAddress(_)));
    }

    #[test]
    fn test_cgn_rejected() {
        let result = validate_outbound_addr("100.64.0.1:4433", &[]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AddressValidationError::PrivateAddress(_)));
    }

    #[test]
    fn test_ipv6_loopback_rejected() {
        let result = validate_outbound_addr("[::1]:4433", &[]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AddressValidationError::PrivateAddress(_)));
    }

    #[test]
    fn test_ipv6_ula_rejected() {
        let result = validate_outbound_addr("[fd00::1]:4433", &[]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AddressValidationError::PrivateAddress(_)));
    }

    #[test]
    fn test_allowlist_overrides() {
        let allowlist = vec!["127.0.0.1:4433".to_string()];
        let result = validate_outbound_addr("127.0.0.1:4433", &allowlist);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "127.0.0.1:4433".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn test_invalid_address_parse_error() {
        let result = validate_outbound_addr("not-an-address", &[]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AddressValidationError::ParseError(_)));
    }

    #[test]
    fn test_ipv6_link_local_rejected() {
        let result = validate_outbound_addr("[fe80::1]:4433", &[]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AddressValidationError::PrivateAddress(_)));
    }

    #[test]
    fn test_unspecified_rejected() {
        let result = validate_outbound_addr("0.0.0.0:4433", &[]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AddressValidationError::PrivateAddress(_)));
    }
}
