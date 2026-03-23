//! Phase 4 hardening: rate limiter stress tests.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};

use mesh_hub::rate_limit::{HubRateLimiter, Operation, RateLimitConfig, RateLimitError};

/// Build a RateLimitConfig with specified per-IP limits and window.
fn make_config(store: u32, query: Option<u32>, window_secs: u64) -> RateLimitConfig {
    let mut ip_limits = HashMap::new();
    ip_limits.insert(Operation::Store, store);
    if let Some(q) = query {
        ip_limits.insert(Operation::Query, q);
    }
    RateLimitConfig {
        ip_limits,
        identity_limits: HashMap::new(),
        window_secs,
    }
}

/// Fire exactly at-limit store checks, all pass. Fire one more, it fails.
#[test]
fn test_rate_limiter_burst() {
    let limiter = HubRateLimiter::new(make_config(10, Some(100), 60));
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));

    for i in 0..10 {
        assert!(
            limiter.check_ip(ip, Operation::Store).is_ok(),
            "request {i} should pass"
        );
    }

    // 11th should fail
    let err = limiter.check_ip(ip, Operation::Store).unwrap_err();
    match err {
        RateLimitError::IpRateLimited {
            ip: err_ip,
            operation,
            limit,
        } => {
            assert_eq!(err_ip, ip);
            assert_eq!(operation, Operation::Store);
            assert_eq!(limit, 10);
        }
        _ => panic!("expected IpRateLimited, got {err:?}"),
    }
}

/// Use a 1-second window, fill it, sleep past the window, and verify
/// requests are accepted again.
#[test]
fn test_rate_limiter_window_expiry() {
    let limiter = HubRateLimiter::new(make_config(3, None, 1));
    let ip = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1));

    assert!(limiter.check_ip(ip, Operation::Store).is_ok());
    assert!(limiter.check_ip(ip, Operation::Store).is_ok());
    assert!(limiter.check_ip(ip, Operation::Store).is_ok());
    assert!(limiter.check_ip(ip, Operation::Store).is_err());

    // Sleep past the 1-second window
    std::thread::sleep(std::time::Duration::from_millis(1100));

    assert!(
        limiter.check_ip(ip, Operation::Store).is_ok(),
        "request should pass after window expires"
    );
}

/// Exhaust the Store limit, then verify Query limit is still available
/// (operations are tracked independently).
#[test]
fn test_rate_limiter_independent_operations() {
    let limiter = HubRateLimiter::new(make_config(5, Some(10), 60));
    let ip = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));

    // Exhaust Store limit
    for _ in 0..5 {
        assert!(limiter.check_ip(ip, Operation::Store).is_ok());
    }
    assert!(
        limiter.check_ip(ip, Operation::Store).is_err(),
        "store should be rate limited"
    );

    // Query should still be fully available
    for i in 0..10 {
        assert!(
            limiter.check_ip(ip, Operation::Query).is_ok(),
            "query {i} should still pass"
        );
    }
    assert!(
        limiter.check_ip(ip, Operation::Query).is_err(),
        "query should be rate limited after 10"
    );
}
