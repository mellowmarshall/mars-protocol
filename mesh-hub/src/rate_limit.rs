//! Per-IP and per-identity sliding-window rate limiter for hub operations.

use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use std::collections::VecDeque;

use mesh_core::identity::Identity;

use crate::storage::redb::identity_bytes;

/// Operation types for rate limiting granularity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation {
    Connect,
    Store,
    Query,
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operation::Connect => write!(f, "connect"),
            Operation::Store => write!(f, "store"),
            Operation::Query => write!(f, "query"),
        }
    }
}

/// Per-operation rate limits configuration.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Max operations per window per IP.
    pub ip_limits: HashMap<Operation, u32>,
    /// Max operations per window per identity.
    pub identity_limits: HashMap<Operation, u32>,
    /// Window duration in seconds.
    pub window_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        let mut ip_limits = HashMap::new();
        ip_limits.insert(Operation::Connect, 60); // 60 connections/min/IP
        ip_limits.insert(Operation::Store, 30); // 30 stores/min/IP
        ip_limits.insert(Operation::Query, 300); // 300 queries/min/IP

        let mut identity_limits = HashMap::new();
        identity_limits.insert(Operation::Store, 20); // 20 stores/min/identity
        identity_limits.insert(Operation::Query, 200); // 200 queries/min/identity

        Self {
            ip_limits,
            identity_limits,
            window_secs: 60,
        }
    }
}

/// Rate limiting errors.
#[derive(Debug)]
pub enum RateLimitError {
    IpRateLimited {
        ip: IpAddr,
        operation: Operation,
        limit: u32,
    },
    IdentityRateLimited {
        did: String,
        operation: Operation,
        limit: u32,
    },
}

impl fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RateLimitError::IpRateLimited {
                ip,
                operation,
                limit,
            } => write!(
                f,
                "IP {ip} rate limited for {operation} (limit: {limit}/window)"
            ),
            RateLimitError::IdentityRateLimited {
                did,
                operation,
                limit,
            } => write!(
                f,
                "identity {did} rate limited for {operation} (limit: {limit}/window)"
            ),
        }
    }
}

impl std::error::Error for RateLimitError {}

/// Monitoring stats for the rate limiter.
#[derive(Debug, Clone)]
pub struct RateLimitStats {
    /// Number of tracked IPs.
    pub tracked_ips: usize,
    /// Number of tracked identities.
    pub tracked_identities: usize,
}

/// Thread-safe sliding window rate limiter for hub operations.
pub struct HubRateLimiter {
    config: RateLimitConfig,
    ip_windows: Mutex<HashMap<IpAddr, HashMap<Operation, VecDeque<Instant>>>>,
    identity_windows: Mutex<HashMap<Vec<u8>, HashMap<Operation, VecDeque<Instant>>>>,
}

impl HubRateLimiter {
    /// Create a new rate limiter with the given configuration.
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            ip_windows: Mutex::new(HashMap::new()),
            identity_windows: Mutex::new(HashMap::new()),
        }
    }

    /// Check whether an IP is within its rate limit for the given operation.
    ///
    /// If under the limit, records the request and returns `Ok(())`.
    /// If over the limit, returns `Err(RateLimitError::IpRateLimited)`.
    pub fn check_ip(&self, ip: IpAddr, op: Operation) -> Result<(), RateLimitError> {
        let limit = match self.config.ip_limits.get(&op) {
            Some(&limit) => limit,
            None => return Ok(()), // no limit configured for this operation
        };

        let window = Duration::from_secs(self.config.window_secs);
        let now = Instant::now();

        let mut ip_windows = self.ip_windows.lock().unwrap();
        let op_windows = ip_windows.entry(ip).or_default();
        let deque = op_windows.entry(op).or_default();

        // Remove entries older than the window
        while let Some(&front) = deque.front() {
            if now.duration_since(front) > window {
                deque.pop_front();
            } else {
                break;
            }
        }

        if deque.len() as u32 >= limit {
            return Err(RateLimitError::IpRateLimited {
                ip,
                operation: op,
                limit,
            });
        }

        deque.push_back(now);
        Ok(())
    }

    /// Check whether an identity is within its rate limit for the given operation.
    ///
    /// If under the limit, records the request and returns `Ok(())`.
    /// If over the limit, returns `Err(RateLimitError::IdentityRateLimited)`.
    pub fn check_identity(
        &self,
        identity: &Identity,
        op: Operation,
    ) -> Result<(), RateLimitError> {
        let limit = match self.config.identity_limits.get(&op) {
            Some(&limit) => limit,
            None => return Ok(()), // no limit configured for this operation
        };

        let window = Duration::from_secs(self.config.window_secs);
        let now = Instant::now();
        let key = identity_bytes(identity);

        let mut identity_windows = self.identity_windows.lock().unwrap();
        let op_windows = identity_windows.entry(key).or_default();
        let deque = op_windows.entry(op).or_default();

        // Remove entries older than the window
        while let Some(&front) = deque.front() {
            if now.duration_since(front) > window {
                deque.pop_front();
            } else {
                break;
            }
        }

        if deque.len() as u32 >= limit {
            return Err(RateLimitError::IdentityRateLimited {
                did: identity.did(),
                operation: op,
                limit,
            });
        }

        deque.push_back(now);
        Ok(())
    }

    /// Convenience method: check both IP and identity limits.
    ///
    /// Returns the first error encountered, checking IP first.
    pub fn check(
        &self,
        ip: IpAddr,
        identity: &Identity,
        op: Operation,
    ) -> Result<(), RateLimitError> {
        self.check_ip(ip, op)?;
        self.check_identity(identity, op)?;
        Ok(())
    }

    /// Remove all entries older than 2x the window duration.
    ///
    /// Intended to be called from a background task to prevent unbounded memory growth.
    pub fn cleanup(&self) {
        let cutoff = Duration::from_secs(self.config.window_secs * 2);
        let now = Instant::now();

        {
            let mut ip_windows = self.ip_windows.lock().unwrap();
            ip_windows.retain(|_ip, op_map| {
                op_map.retain(|_op, deque| {
                    while let Some(&front) = deque.front() {
                        if now.duration_since(front) > cutoff {
                            deque.pop_front();
                        } else {
                            break;
                        }
                    }
                    !deque.is_empty()
                });
                !op_map.is_empty()
            });
        }

        {
            let mut identity_windows = self.identity_windows.lock().unwrap();
            identity_windows.retain(|_id, op_map| {
                op_map.retain(|_op, deque| {
                    while let Some(&front) = deque.front() {
                        if now.duration_since(front) > cutoff {
                            deque.pop_front();
                        } else {
                            break;
                        }
                    }
                    !deque.is_empty()
                });
                !op_map.is_empty()
            });
        }
    }

    /// Return monitoring stats for the rate limiter.
    pub fn stats(&self) -> RateLimitStats {
        let tracked_ips = self.ip_windows.lock().unwrap().len();
        let tracked_identities = self.identity_windows.lock().unwrap().len();
        RateLimitStats {
            tracked_ips,
            tracked_identities,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::identity::Keypair;
    use std::net::Ipv4Addr;

    fn small_config() -> RateLimitConfig {
        let mut ip_limits = HashMap::new();
        ip_limits.insert(Operation::Store, 3);
        ip_limits.insert(Operation::Query, 5);
        ip_limits.insert(Operation::Connect, 2);

        let mut identity_limits = HashMap::new();
        identity_limits.insert(Operation::Store, 2);
        identity_limits.insert(Operation::Query, 4);

        RateLimitConfig {
            ip_limits,
            identity_limits,
            window_secs: 60,
        }
    }

    #[test]
    fn test_ip_rate_limit_allows_under_threshold() {
        let limiter = HubRateLimiter::new(small_config());
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        // Store limit is 3 — first 3 should pass
        assert!(limiter.check_ip(ip, Operation::Store).is_ok());
        assert!(limiter.check_ip(ip, Operation::Store).is_ok());
        assert!(limiter.check_ip(ip, Operation::Store).is_ok());
    }

    #[test]
    fn test_ip_rate_limit_blocks_over_threshold() {
        let limiter = HubRateLimiter::new(small_config());
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        // Store limit is 3 — 4th should fail
        assert!(limiter.check_ip(ip, Operation::Store).is_ok());
        assert!(limiter.check_ip(ip, Operation::Store).is_ok());
        assert!(limiter.check_ip(ip, Operation::Store).is_ok());

        let err = limiter.check_ip(ip, Operation::Store).unwrap_err();
        match err {
            RateLimitError::IpRateLimited {
                ip: err_ip,
                operation,
                limit,
            } => {
                assert_eq!(err_ip, ip);
                assert_eq!(operation, Operation::Store);
                assert_eq!(limit, 3);
            }
            _ => panic!("expected IpRateLimited"),
        }
    }

    #[test]
    fn test_identity_rate_limit_blocks_over_threshold() {
        let limiter = HubRateLimiter::new(small_config());
        let kp = Keypair::generate();
        let identity = kp.identity();

        // Identity store limit is 2 — 3rd should fail
        assert!(limiter.check_identity(&identity, Operation::Store).is_ok());
        assert!(limiter.check_identity(&identity, Operation::Store).is_ok());

        let err = limiter
            .check_identity(&identity, Operation::Store)
            .unwrap_err();
        match err {
            RateLimitError::IdentityRateLimited {
                did,
                operation,
                limit,
            } => {
                assert_eq!(did, identity.did());
                assert_eq!(operation, Operation::Store);
                assert_eq!(limit, 2);
            }
            _ => panic!("expected IdentityRateLimited"),
        }
    }

    #[test]
    fn test_different_operations_tracked_separately() {
        let limiter = HubRateLimiter::new(small_config());
        let ip = IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1));

        // Exhaust Store limit (3)
        assert!(limiter.check_ip(ip, Operation::Store).is_ok());
        assert!(limiter.check_ip(ip, Operation::Store).is_ok());
        assert!(limiter.check_ip(ip, Operation::Store).is_ok());
        assert!(limiter.check_ip(ip, Operation::Store).is_err());

        // Query should still work (limit 5, separate tracking)
        assert!(limiter.check_ip(ip, Operation::Query).is_ok());
        assert!(limiter.check_ip(ip, Operation::Query).is_ok());
        assert!(limiter.check_ip(ip, Operation::Query).is_ok());
        assert!(limiter.check_ip(ip, Operation::Query).is_ok());
        assert!(limiter.check_ip(ip, Operation::Query).is_ok());
        assert!(limiter.check_ip(ip, Operation::Query).is_err());
    }

    #[test]
    fn test_cleanup_removes_stale_entries() {
        // Use a very short window so entries expire quickly
        let config = RateLimitConfig {
            ip_limits: {
                let mut m = HashMap::new();
                m.insert(Operation::Store, 100);
                m
            },
            identity_limits: {
                let mut m = HashMap::new();
                m.insert(Operation::Store, 100);
                m
            },
            window_secs: 0, // 0-second window means everything is immediately stale
        };
        let limiter = HubRateLimiter::new(config);
        let ip = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        let kp = Keypair::generate();
        let identity = kp.identity();

        // Add some entries
        assert!(limiter.check_ip(ip, Operation::Store).is_ok());
        assert!(limiter.check_identity(&identity, Operation::Store).is_ok());

        let stats_before = limiter.stats();
        assert_eq!(stats_before.tracked_ips, 1);
        assert_eq!(stats_before.tracked_identities, 1);

        // Cleanup should remove everything (window_secs=0, so 2x window = 0)
        limiter.cleanup();

        let stats_after = limiter.stats();
        assert_eq!(stats_after.tracked_ips, 0);
        assert_eq!(stats_after.tracked_identities, 0);
    }

    #[test]
    fn test_check_both_ip_and_identity() {
        let limiter = HubRateLimiter::new(small_config());
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1));
        let kp = Keypair::generate();
        let identity = kp.identity();

        // Both under limit — should pass
        assert!(limiter.check(ip, &identity, Operation::Store).is_ok());
        assert!(limiter.check(ip, &identity, Operation::Store).is_ok());

        // Identity limit (2) hit before IP limit (3) — should fail with identity error
        let err = limiter.check(ip, &identity, Operation::Store).unwrap_err();
        match err {
            RateLimitError::IdentityRateLimited { .. } => {} // expected
            _ => panic!("expected IdentityRateLimited, got {err:?}"),
        }

        // Use a different identity — IP limit (3) should now be hit
        // IP already has 2 successful check_ip calls from the `check` calls above,
        // plus one more from the third `check` call (IP check passes before identity fails).
        // So IP is at 3/3 — next should fail.
        let kp2 = Keypair::generate();
        let identity2 = kp2.identity();
        let err = limiter
            .check(ip, &identity2, Operation::Store)
            .unwrap_err();
        match err {
            RateLimitError::IpRateLimited { .. } => {} // expected
            _ => panic!("expected IpRateLimited, got {err:?}"),
        }
    }
}
