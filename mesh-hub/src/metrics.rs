//! Aggregate Prometheus metrics for the mesh hub.
//!
//! **Security constraint (Security #8 — Metadata Leak Prevention):**
//! All metrics are aggregate only. No tenant_id or publisher DID labels are used
//! to prevent behavioral profiling surfaces.

use std::sync::Arc;

use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use prometheus_client::registry::Registry;

/// Label set for rate-limit counters (keyed by operation name only — no tenant/DID).
#[derive(Clone, Debug, Hash, PartialEq, Eq, prometheus_client::encoding::EncodeLabelSet)]
pub struct OperationLabel {
    pub operation: String,
}

/// All hub metrics, registered in a single Prometheus registry.
#[derive(Clone)]
pub struct HubMetrics {
    pub registry: Arc<std::sync::Mutex<Registry>>,

    // DHT operations
    pub stores_total: Counter,
    pub queries_total: Counter,
    pub store_duration_seconds: Histogram,
    pub query_duration_seconds: Histogram,

    // Storage
    pub descriptors_total: Gauge,
    pub evictions_total: Counter,

    // Peering
    pub peers_connected: Gauge,
    pub gossip_rounds_total: Counter,

    // Rate limiting (per-operation, aggregate — no tenant/DID labels)
    pub rate_limited_total: Family<OperationLabel, Counter>,

    // Transport
    pub connections_active: Gauge,
}

impl HubMetrics {
    /// Create and register all metrics. Call once at startup.
    pub fn new() -> Self {
        let mut registry = Registry::default();

        let stores_total = Counter::default();
        registry.register(
            "mesh_hub_stores_total",
            "Total STORE operations processed",
            stores_total.clone(),
        );

        let queries_total = Counter::default();
        registry.register(
            "mesh_hub_queries_total",
            "Total FIND_VALUE queries processed",
            queries_total.clone(),
        );

        // Latency buckets: 100us to ~26s (exponential)
        let store_duration_seconds =
            Histogram::new(exponential_buckets(0.0001, 2.0, 18));
        registry.register(
            "mesh_hub_store_duration_seconds",
            "STORE processing latency",
            store_duration_seconds.clone(),
        );

        let query_duration_seconds =
            Histogram::new(exponential_buckets(0.0001, 2.0, 18));
        registry.register(
            "mesh_hub_query_duration_seconds",
            "FIND_VALUE processing latency",
            query_duration_seconds.clone(),
        );

        let descriptors_total = Gauge::default();
        registry.register(
            "mesh_hub_descriptors_total",
            "Current descriptor count",
            descriptors_total.clone(),
        );

        let evictions_total = Counter::default();
        registry.register(
            "mesh_hub_evictions_total",
            "Descriptors evicted",
            evictions_total.clone(),
        );

        let peers_connected = Gauge::default();
        registry.register(
            "mesh_hub_peers_connected",
            "Connected peer hubs",
            peers_connected.clone(),
        );

        let gossip_rounds_total = Counter::default();
        registry.register(
            "mesh_hub_gossip_rounds_total",
            "Gossip rounds completed",
            gossip_rounds_total.clone(),
        );

        let rate_limited_total = Family::<OperationLabel, Counter>::default();
        registry.register(
            "mesh_hub_rate_limited_total",
            "Requests rejected by rate limiter",
            rate_limited_total.clone(),
        );

        let connections_active = Gauge::default();
        registry.register(
            "mesh_hub_connections_active",
            "Active QUIC connections",
            connections_active.clone(),
        );

        Self {
            registry: Arc::new(std::sync::Mutex::new(registry)),
            stores_total,
            queries_total,
            store_duration_seconds,
            query_duration_seconds,
            descriptors_total,
            evictions_total,
            peers_connected,
            gossip_rounds_total,
            rate_limited_total,
            connections_active,
        }
    }

    /// Record a completed STORE operation with its latency.
    pub fn record_store(&self, duration_secs: f64) {
        self.stores_total.inc();
        self.store_duration_seconds.observe(duration_secs);
    }

    /// Record a completed FIND_VALUE query with its latency.
    pub fn record_query(&self, duration_secs: f64) {
        self.queries_total.inc();
        self.query_duration_seconds.observe(duration_secs);
    }

    /// Record a rate-limited request (aggregate, keyed by operation name only).
    pub fn record_rate_limited(&self, operation: &str) {
        self.rate_limited_total
            .get_or_create(&OperationLabel {
                operation: operation.to_string(),
            })
            .inc();
    }

    /// Set the current descriptor count gauge.
    pub fn set_descriptor_count(&self, count: i64) {
        self.descriptors_total.set(count);
    }

    /// Set the current connected-peers gauge.
    pub fn set_peers_connected(&self, count: i64) {
        self.peers_connected.set(count);
    }

    /// Render all metrics in Prometheus text exposition format.
    pub fn render(&self) -> String {
        let mut buf = String::new();
        let registry = self.registry.lock().unwrap();
        encode(&mut buf, &registry).expect("prometheus encoding should not fail");
        buf
    }
}

impl Default for HubMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_init_does_not_panic() {
        let m = HubMetrics::new();
        // Render should produce valid output
        let output = m.render();
        // The output should either be empty (no observations yet) or contain our metric names.
        assert!(output.is_empty() || output.contains("mesh_hub"));
    }

    #[test]
    fn counter_increments_are_reflected_in_render() {
        let m = HubMetrics::new();
        m.record_store(0.005);
        m.record_store(0.010);
        m.record_query(0.001);
        m.record_rate_limited("store");
        m.record_rate_limited("store");
        m.record_rate_limited("query");
        m.set_descriptor_count(42);
        m.set_peers_connected(3);

        let output = m.render();
        // Verify counters appear in output
        assert!(output.contains("mesh_hub_stores_total"), "stores_total missing");
        assert!(output.contains("mesh_hub_queries_total"), "queries_total missing");
        assert!(output.contains("mesh_hub_rate_limited_total"), "rate_limited_total missing");
        assert!(output.contains("mesh_hub_descriptors_total"), "descriptors_total missing");
        assert!(output.contains("mesh_hub_peers_connected"), "peers_connected missing");
    }

    #[test]
    fn rate_limited_labels_are_aggregate_only() {
        let m = HubMetrics::new();
        m.record_rate_limited("store");
        m.record_rate_limited("query");

        let output = m.render();
        // Must NOT contain any tenant_id or DID label (Security #8)
        assert!(!output.contains("tenant_id"), "tenant_id label must not appear");
        assert!(!output.contains("publisher"), "publisher label must not appear");
        // Should contain operation labels only
        assert!(output.contains("operation=\"store\""));
        assert!(output.contains("operation=\"query\""));
    }
}
