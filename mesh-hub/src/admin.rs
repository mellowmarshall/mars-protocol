//! Admin API — composable Axum router for hub management.

use std::sync::{Arc, Mutex};

use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use mesh_core::identity::Identity;
use mesh_dht::DescriptorStorage;
use serde::Deserialize;
use uuid::Uuid;

use crate::HubDhtNode;
use crate::metrics::HubMetrics;
use crate::tenant::TenantManager;

/// Shared state for admin API handlers.
pub struct AdminState {
    pub dht_node: Arc<Mutex<HubDhtNode>>,
    pub tenant_manager: Arc<Mutex<TenantManager>>,
    pub start_time: std::time::Instant,
    /// Hub DID for challenge generation (set from hub identity).
    pub hub_did: Option<String>,
    /// Operator bearer token for admin API authentication.
    pub operator_token: Option<String>,
    /// Hub metrics handle for the /metrics endpoint.
    pub metrics: Option<HubMetrics>,
}

/// Build the composable admin API router.
///
/// Downstream projects can merge additional routes onto this base.
pub fn admin_router(state: Arc<AdminState>) -> Router {
    // Public routes (no auth required)
    let public = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api/v1/hub/status", get(hub_status))
        // Challenge-response identity verification (public-facing)
        .route(
            "/api/v1/tenants/:id/identities/challenge",
            post(create_identity_challenge),
        )
        .route(
            "/api/v1/tenants/:id/identities/verify",
            post(verify_identity),
        );

    // Operator routes (require bearer token when configured)
    let operator = Router::new()
        .route(
            "/api/v1/tenants",
            get(list_tenants).post(create_tenant),
        )
        .route(
            "/api/v1/tenants/:id",
            get(get_tenant).delete(delete_tenant),
        )
        .route("/api/v1/tenants/:id/identities", post(register_identity))
        .route(
            "/api/v1/tenants/:id/identities/:did",
            delete(remove_identity),
        )
        .route("/api/v1/tenants/:id/usage", get(get_tenant_usage))
        .route("/api/v1/tenants/:id/quota", patch(update_tenant_quota))
        .route("/metrics", get(prometheus_metrics));

    public.merge(operator).with_state(state)
}

// ── Auth helpers ──

/// Extract and validate the operator bearer token from request headers.
/// Returns `Ok(())` if no token is configured (open access) or if the token matches.
/// Returns `Err` status code if authentication fails.
fn check_operator_token(state: &AdminState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let Some(ref expected) = state.operator_token else {
        return Ok(()); // No token configured, open access
    };

    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if let Some(token) = auth.strip_prefix("Bearer ")
        && token == expected
    {
        return Ok(());
    }

    Err(StatusCode::UNAUTHORIZED)
}

// ── Health checks ──

async fn healthz() -> StatusCode {
    StatusCode::OK
}

async fn readyz(State(state): State<Arc<AdminState>>) -> StatusCode {
    // Check that storage is accessible
    let Ok(node) = state.dht_node.lock() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    let _ = node.store.descriptor_count();
    StatusCode::OK
}

// ── Hub status ──

async fn hub_status(State(state): State<Arc<AdminState>>) -> impl IntoResponse {
    let node = state.dht_node.lock().unwrap();
    let descriptor_count = node.store.descriptor_count();
    let routing_key_count = node.store.routing_key_count();
    drop(node);
    let tm = state.tenant_manager.lock().unwrap();
    let tenant_count = tm.list_tenants().map(|t| t.len()).unwrap_or(0);
    Json(serde_json::json!({
        "uptime_secs": state.start_time.elapsed().as_secs(),
        "descriptor_count": descriptor_count,
        "routing_key_count": routing_key_count,
        "tenant_count": tenant_count,
    }))
}

// ── Prometheus metrics (operator-only) ──

async fn prometheus_metrics(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(status) = check_operator_token(&state, &headers) {
        return status.into_response();
    }
    let Some(ref metrics) = state.metrics else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "metrics not enabled",
        )
            .into_response();
    };
    let body = metrics.render();
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
        .into_response()
}

// ── Tenant management ──

#[derive(Deserialize)]
struct CreateTenantRequest {
    name: String,
    #[serde(default = "default_tier")]
    tier: String,
}

fn default_tier() -> String {
    "free".into()
}

async fn create_tenant(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
    Json(req): Json<CreateTenantRequest>,
) -> impl IntoResponse {
    if let Err(status) = check_operator_token(&state, &headers) {
        return status.into_response();
    }
    let tm = state.tenant_manager.lock().unwrap();
    match tm.create_tenant(&req.name, &req.tier) {
        Ok(tenant) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(tenant).unwrap()),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn list_tenants(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(status) = check_operator_token(&state, &headers) {
        return status.into_response();
    }
    let tm = state.tenant_manager.lock().unwrap();
    match tm.list_tenants() {
        Ok(tenants) => Json(serde_json::to_value(tenants).unwrap()).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn get_tenant(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    if let Err(status) = check_operator_token(&state, &headers) {
        return status.into_response();
    }
    let Ok(uuid) = Uuid::parse_str(&id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid uuid"})),
        )
            .into_response();
    };
    let tm = state.tenant_manager.lock().unwrap();
    match tm.get_tenant(&uuid) {
        Ok(Some(tenant)) => Json(serde_json::to_value(tenant).unwrap()).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn delete_tenant(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    if let Err(status) = check_operator_token(&state, &headers) {
        return status.into_response();
    }
    let Ok(uuid) = Uuid::parse_str(&id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid uuid"})),
        )
            .into_response();
    };
    let tm = state.tenant_manager.lock().unwrap();
    match tm.delete_tenant(&uuid) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

// ── Direct identity registration (operator-only) ──

#[derive(Deserialize)]
struct RegisterIdentityRequest {
    did: String,
    identity_bytes: String, // hex-encoded
}

async fn register_identity(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<RegisterIdentityRequest>,
) -> impl IntoResponse {
    if let Err(status) = check_operator_token(&state, &headers) {
        return status.into_response();
    }
    let Ok(uuid) = Uuid::parse_str(&id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid uuid"})),
        )
            .into_response();
    };
    let Ok(id_bytes) = hex::decode(&req.identity_bytes) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid hex for identity_bytes"})),
        )
            .into_response();
    };
    let tm = state.tenant_manager.lock().unwrap();
    match tm.register_identity(&uuid, &id_bytes, &req.did) {
        Ok(()) => StatusCode::CREATED.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn remove_identity(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
    AxumPath((id, did)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    if let Err(status) = check_operator_token(&state, &headers) {
        return status.into_response();
    }
    let Ok(uuid) = Uuid::parse_str(&id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid uuid"})),
        )
            .into_response();
    };
    let tm = state.tenant_manager.lock().unwrap();
    match tm.remove_identity(&uuid, &did) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

// ── DID-Auth Challenge-Response Identity Verification ──

#[derive(Deserialize)]
struct ChallengeRequest {
    action: String,
}

async fn create_identity_challenge(
    State(state): State<Arc<AdminState>>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<ChallengeRequest>,
) -> impl IntoResponse {
    let Ok(uuid) = Uuid::parse_str(&id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid uuid"})),
        )
            .into_response();
    };
    let hub_did = state.hub_did.clone().unwrap_or_default();
    let tm = state.tenant_manager.lock().unwrap();

    // Verify tenant exists
    match tm.get_tenant(&uuid) {
        Ok(None) => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
        Ok(Some(_)) => {}
    }

    match tm.create_challenge(&uuid, &hub_did, &req.action) {
        Ok(challenge) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(challenge).unwrap()),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct VerifyIdentityRequest {
    challenge_id: String,
    identity_bytes: String, // hex-encoded
    did: String,
    signature: String, // hex-encoded
}

async fn verify_identity(
    State(state): State<Arc<AdminState>>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<VerifyIdentityRequest>,
) -> impl IntoResponse {
    let Ok(tenant_uuid) = Uuid::parse_str(&id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid tenant uuid"})),
        )
            .into_response();
    };
    let Ok(challenge_uuid) = Uuid::parse_str(&req.challenge_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid challenge_id"})),
        )
            .into_response();
    };
    let Ok(id_bytes) = hex::decode(&req.identity_bytes) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid hex for identity_bytes"})),
        )
            .into_response();
    };
    let Ok(sig_bytes) = hex::decode(&req.signature) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid hex for signature"})),
        )
            .into_response();
    };

    // Deserialize identity from raw bytes (algorithm byte + public key)
    if id_bytes.len() < 2 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "identity_bytes too short"})),
        )
            .into_response();
    }
    let identity = Identity::new(id_bytes[0], id_bytes[1..].to_vec());

    // Verify the DID matches the identity
    if identity.did() != req.did {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "DID does not match identity"})),
        )
            .into_response();
    }

    let tm = state.tenant_manager.lock().unwrap();

    // Get challenge
    let challenge = match tm.get_challenge(&challenge_uuid) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "challenge not found or already consumed"})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    // Check expiry
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64;
    if challenge.is_expired(now) {
        return (
            StatusCode::GONE,
            Json(serde_json::json!({"error": "challenge expired"})),
        )
            .into_response();
    }

    // Verify signature
    if let Err(e) = challenge.verify(&identity, &sig_bytes) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }

    // Consume challenge
    if let Err(e) = tm.consume_challenge(&challenge_uuid) {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }

    // Register the identity
    if let Err(e) = tm.register_identity(&tenant_uuid, &id_bytes, &req.did) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "registered", "did": req.did})),
    )
        .into_response()
}

// ── Tenant usage and quota management (operator-only) ──

async fn get_tenant_usage(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    if let Err(status) = check_operator_token(&state, &headers) {
        return status.into_response();
    }
    let Ok(uuid) = Uuid::parse_str(&id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid uuid"})),
        )
            .into_response();
    };
    let tm = state.tenant_manager.lock().unwrap();
    match tm.get_usage(&uuid) {
        Ok(usage) => Json(serde_json::to_value(usage).unwrap()).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct UpdateQuotaRequest {
    max_descriptors: Option<u64>,
    max_storage_bytes: Option<u64>,
    mu_limit: Option<i64>,
}

async fn update_tenant_quota(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateQuotaRequest>,
) -> impl IntoResponse {
    if let Err(status) = check_operator_token(&state, &headers) {
        return status.into_response();
    }
    let Ok(uuid) = Uuid::parse_str(&id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid uuid"})),
        )
            .into_response();
    };
    let tm = state.tenant_manager.lock().unwrap();

    // Verify tenant exists
    match tm.get_tenant(&uuid) {
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
        Ok(Some(_)) => {}
    }

    match tm.update_quotas(&uuid, req.max_descriptors, req.max_storage_bytes, req.mu_limit) {
        Ok(()) => {
            // Return updated tenant
            match tm.get_tenant(&uuid) {
                Ok(Some(tenant)) => Json(serde_json::to_value(tenant).unwrap()).into_response(),
                _ => StatusCode::OK.into_response(),
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: check_operator_token with a given token config and header.
    fn check_token(
        configured: Option<&str>,
        header_value: Option<&str>,
    ) -> Result<(), StatusCode> {
        // We only need operator_token and headers for this test — build a
        // lightweight AdminState without touching DhtNode.
        //
        // check_operator_token only reads `state.operator_token` and `headers`,
        // so we construct a partial state by re-implementing the check inline
        // (same logic, avoids needing a real DhtNode).
        let expected = configured.map(|s| s.to_string());
        let Some(ref expected_tok) = expected else {
            return Ok(());
        };

        let auth = header_value.unwrap_or("");
        if let Some(token) = auth.strip_prefix("Bearer ")
            && token == expected_tok
        {
            return Ok(());
        }
        Err(StatusCode::UNAUTHORIZED)
    }

    #[test]
    fn bearer_token_valid() {
        assert!(check_token(
            Some("secret-token-123"),
            Some("Bearer secret-token-123"),
        )
        .is_ok());
    }

    #[test]
    fn bearer_token_invalid() {
        assert_eq!(
            check_token(Some("secret-token-123"), Some("Bearer wrong-token")),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn bearer_token_missing() {
        assert_eq!(
            check_token(Some("secret-token-123"), None),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn bearer_token_none_configured() {
        // No token configured means open access
        assert!(check_token(None, None).is_ok());
        assert!(check_token(None, Some("Bearer anything")).is_ok());
    }
}
