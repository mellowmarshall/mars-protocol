//! Admin API — composable Axum router for hub management.

use std::sync::{Arc, Mutex};

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use mesh_dht::DescriptorStorage;
use serde::Deserialize;
use uuid::Uuid;

use crate::HubDhtNode;
use crate::tenant::TenantManager;

/// Shared state for admin API handlers.
pub struct AdminState {
    pub dht_node: Arc<Mutex<HubDhtNode>>,
    pub tenant_manager: Arc<Mutex<TenantManager>>,
    pub start_time: std::time::Instant,
}

/// Build the composable admin API router.
///
/// Downstream projects can merge additional routes onto this base.
pub fn admin_router(state: Arc<AdminState>) -> Router {
    Router::new()
        // Health checks
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        // Hub status
        .route("/api/v1/hub/status", get(hub_status))
        // Tenant management
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
        .with_state(state)
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
    Json(req): Json<CreateTenantRequest>,
) -> impl IntoResponse {
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

async fn list_tenants(State(state): State<Arc<AdminState>>) -> impl IntoResponse {
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
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
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
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
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

#[derive(Deserialize)]
struct RegisterIdentityRequest {
    did: String,
    identity_bytes: String, // hex-encoded
}

async fn register_identity(
    State(state): State<Arc<AdminState>>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<RegisterIdentityRequest>,
) -> impl IntoResponse {
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
    AxumPath((id, did)): AxumPath<(String, String)>,
) -> impl IntoResponse {
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
