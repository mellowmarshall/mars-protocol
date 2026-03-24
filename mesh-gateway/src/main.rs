//! `mesh-gateway` — HTTP/JSON gateway for the Capability Mesh Protocol.
//!
//! Exposes REST endpoints so non-Rust agents (Python, TypeScript, Go) can
//! publish and discover capabilities on the mesh via simple HTTP calls.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

use mesh_client::MeshClient;
use mesh_core::hash::schema_hash;
use mesh_core::identity::Keypair;
use mesh_core::message::NodeAddr;
use mesh_core::routing::hierarchical_routing_keys;
use mesh_core::Descriptor;

// ── CLI ─────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "mesh-gateway", about = "HTTP/JSON gateway for the Capability Mesh Protocol")]
struct Cli {
    /// Mesh seed node address (e.g. "1.2.3.4:4433")
    #[arg(long)]
    seed: String,

    /// HTTP listen address
    #[arg(long, default_value = "0.0.0.0:3000")]
    listen: String,

    /// Path to Ed25519 secret key file (32 raw bytes). Generates ephemeral if omitted.
    #[arg(long)]
    identity: Option<String>,
}

// ── Shared state ────────────────────────────────────────────────────────

struct AppState {
    client: Mutex<MeshClient>,
    keypair: Keypair,
    seed: NodeAddr,
}

// ── Request / response types ────────────────────────────────────────────

#[derive(Deserialize)]
struct PublishRequest {
    r#type: String,
    endpoint: String,
    #[serde(default)]
    params: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct PublishResponse {
    ok: bool,
    descriptor_id: String,
}

#[derive(Deserialize)]
struct DiscoverQuery {
    r#type: String,
}

#[derive(Serialize)]
struct DiscoverResponse {
    descriptors: Vec<DescriptorJson>,
}

#[derive(Serialize)]
struct DescriptorJson {
    id: String,
    publisher: String,
    r#type: String,
    endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
    timestamp: u64,
    ttl: u32,
    sequence: u64,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    identity: String,
    seed: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

// ── Payload CBOR encoding ───────────────────────────────────────────────

/// The capability payload stored inside a descriptor.
#[derive(Serialize, Deserialize)]
struct CapabilityPayload {
    r#type: String,
    endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

fn encode_capability_payload(
    cap_type: &str,
    endpoint: &str,
    params: Option<&serde_json::Value>,
) -> Result<Vec<u8>, String> {
    let payload = CapabilityPayload {
        r#type: cap_type.to_string(),
        endpoint: endpoint.to_string(),
        params: params.cloned(),
    };
    // Encode as JSON bytes for the descriptor payload.
    // We use JSON rather than CBOR for the inner payload so that
    // any language can trivially decode it.
    serde_json::to_vec(&payload).map_err(|e| e.to_string())
}

fn decode_capability_payload(data: &[u8]) -> Result<CapabilityPayload, String> {
    serde_json::from_slice(data).map_err(|e| e.to_string())
}

// ── Handlers ────────────────────────────────────────────────────────────

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        identity: state.keypair.identity().did(),
        seed: state.seed.address.clone(),
    })
}

async fn publish(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PublishRequest>,
) -> Result<Json<PublishResponse>, (StatusCode, Json<ErrorResponse>)> {
    let payload = encode_capability_payload(&req.r#type, &req.endpoint, req.params.as_ref())
        .map_err(|e| {
            error!(error = %e, "failed to encode payload");
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("payload encoding failed: {e}"),
                }),
            )
        })?;

    let routing_keys = hierarchical_routing_keys(&req.r#type);
    let now = now_micros();

    let descriptor = Descriptor::create(
        &state.keypair,
        schema_hash("core/capability"),
        req.r#type.clone(),
        payload,
        now,
        1,
        3600,
        routing_keys,
    )
    .map_err(|e| {
        error!(error = %e, "failed to create descriptor");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("descriptor creation failed: {e}"),
            }),
        )
    })?;

    let descriptor_id = format!("blake3:{}", descriptor.id.to_hex());

    let mut client = state.client.lock().await;
    let ack = client
        .publish(descriptor, &state.seed)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to publish descriptor");
            (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse {
                    error: format!("mesh publish failed: {e}"),
                }),
            )
        })?;

    if !ack.stored {
        let reason = ack.reason.unwrap_or_else(|| "unknown".to_string());
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse {
                error: format!("mesh node rejected descriptor: {reason}"),
            }),
        ));
    }

    info!(descriptor_id = %descriptor_id, "published descriptor");
    Ok(Json(PublishResponse {
        ok: true,
        descriptor_id,
    }))
}

async fn discover(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DiscoverQuery>,
) -> Result<Json<DiscoverResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rk = mesh_core::routing::routing_key(&query.r#type);

    let mut client = state.client.lock().await;
    let descriptors = client.discover(&rk).await.map_err(|e| {
        error!(error = %e, "discover failed");
        (
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse {
                error: format!("mesh discover failed: {e}"),
            }),
        )
    })?;

    let results: Vec<DescriptorJson> = descriptors
        .into_iter()
        .filter_map(|d| descriptor_to_json(d).ok())
        .collect();

    info!(count = results.len(), query_type = %query.r#type, "discover results");
    Ok(Json(DiscoverResponse {
        descriptors: results,
    }))
}

fn descriptor_to_json(d: Descriptor) -> Result<DescriptorJson, String> {
    let cap = decode_capability_payload(&d.payload)?;
    Ok(DescriptorJson {
        id: format!("blake3:{}", d.id.to_hex()),
        publisher: d.publisher.did(),
        r#type: cap.r#type,
        endpoint: cap.endpoint,
        params: cap.params,
        timestamp: d.timestamp,
        ttl: d.ttl,
        sequence: d.sequence,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

fn load_or_generate_keypair(path: Option<&str>) -> Result<Keypair, Box<dyn std::error::Error>> {
    match path {
        Some(p) => {
            let bytes = std::fs::read(p)?;
            if bytes.len() != 32 {
                return Err(format!(
                    "identity file must be exactly 32 bytes, got {}",
                    bytes.len()
                )
                .into());
            }
            let secret: [u8; 32] = bytes.try_into().unwrap();
            Ok(Keypair::from_bytes(&secret))
        }
        None => {
            info!("no identity file provided, generating ephemeral keypair");
            Ok(Keypair::generate())
        }
    }
}

// ── Main ────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mesh_gateway=info".into()),
        )
        .init();

    let cli = Cli::parse();

    // Load or generate the keypair, then derive two instances from the same
    // secret (MeshClient takes ownership, and we need one for signing).
    let keypair = load_or_generate_keypair(cli.identity.as_deref())?;
    let secret = keypair.secret_bytes();
    let client_keypair = Keypair::from_bytes(&secret);
    let state_keypair = Keypair::from_bytes(&secret);

    let identity_did = state_keypair.identity().did();
    info!(identity = %identity_did, "loaded identity");

    // Bind the QUIC transport to an ephemeral port for DHT communication
    let quic_bind: SocketAddr = "0.0.0.0:0".parse()?;

    let mut client = MeshClient::new(client_keypair, quic_bind)
        .await
        .map_err(|e| format!("failed to create mesh client: {e}"))?;

    let seed_addr = NodeAddr::quic(&cli.seed);

    info!(seed = %cli.seed, "bootstrapping from seed node");
    let discovered = client
        .bootstrap(&[seed_addr.clone()])
        .await
        .map_err(|e| format!("bootstrap failed: {e}"))?;
    info!(discovered, "bootstrap complete");

    let state = Arc::new(AppState {
        client: Mutex::new(client),
        keypair: state_keypair,
        seed: seed_addr,
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/publish", post(publish))
        .route("/v1/discover", get(discover))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listen_addr: SocketAddr = cli.listen.parse()?;
    info!(listen = %listen_addr, "starting HTTP server");
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
