//! `mesh-node` — CLI binary that wires together mesh-core, mesh-transport,
//! and mesh-dht into a working mesh node.
//!
//! Demonstrates the full Capability Mesh Protocol end-to-end:
//! node startup, bootstrap, capability publish/discover, and ping.

mod transport;

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use mesh_core::frame::{
    MSG_FIND_NODE, MSG_FIND_NODE_RESULT, MSG_FIND_VALUE, MSG_FIND_VALUE_RESULT, MSG_PING, MSG_PONG,
    MSG_STORE, MSG_STORE_ACK,
};
use mesh_core::hash::schema_hash;
use mesh_core::identity::Keypair;
use mesh_core::message::{
    FindNode, FindValue, FindValueResult, NodeAddr, Ping, Pong, Store, StoreAck, from_cbor, to_cbor,
};
use mesh_core::routing::{hierarchical_routing_keys, routing_key};
use mesh_core::{Descriptor, Frame};
use mesh_dht::DhtNode;
use mesh_dht::node::DhtConfig;
use mesh_transport::MeshEndpoint;
use tokio::sync::Mutex;
use tracing::error;

use crate::transport::QuicTransport;

#[derive(Parser)]
#[command(name = "mesh-node", about = "Capability Mesh Protocol node")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a mesh node and listen for connections
    Start {
        /// Bind address (default: 0.0.0.0:4433)
        #[arg(long, default_value = "0.0.0.0:4433")]
        listen: String,
        /// Seed node address to bootstrap from (repeatable)
        #[arg(long)]
        seed: Vec<String>,
        /// Path to identity key file (default: generate new)
        #[arg(long)]
        identity: Option<PathBuf>,
    },
    /// Publish a capability descriptor to the mesh
    Publish {
        /// Capability type (e.g., compute/inference/text-generation)
        #[arg(long = "type")]
        cap_type: String,
        /// Provider's endpoint address
        #[arg(long)]
        endpoint: String,
        /// Optional JSON params
        #[arg(long)]
        params: Option<String>,
        /// Seed node to connect to for publishing
        #[arg(long)]
        seed: String,
        /// Path to identity key file
        #[arg(long)]
        identity: Option<PathBuf>,
    },
    /// Discover capabilities on the mesh
    Discover {
        /// Capability type to search for
        #[arg(long = "type")]
        cap_type: String,
        /// Seed node to connect to
        #[arg(long)]
        seed: String,
        /// Path to identity key file
        #[arg(long)]
        identity: Option<PathBuf>,
    },
    /// Ping a remote mesh node
    Ping {
        /// Target address
        #[arg(long)]
        addr: String,
        /// Path to identity key file
        #[arg(long)]
        identity: Option<PathBuf>,
    },
    /// Generate or show identity
    Identity {
        /// Generate a new keypair
        #[arg(long)]
        generate: bool,
        /// Path to save/load the key
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

fn now_micros() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Load or generate a keypair.
fn load_or_generate_keypair(path: Option<&PathBuf>) -> Keypair {
    if let Some(p) = path {
        if p.exists() {
            let bytes = std::fs::read(p).expect("failed to read identity file");
            let secret: [u8; 32] = bytes
                .try_into()
                .expect("identity file must be exactly 32 bytes");
            return Keypair::from_bytes(&secret);
        }
        // Generate and save
        let kp = Keypair::generate();
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("failed to create identity directory");
        }
        std::fs::write(p, kp.secret_bytes()).expect("failed to write identity file");
        kp
    } else {
        Keypair::generate()
    }
}

fn parse_addr(s: &str) -> SocketAddr {
    s.parse().unwrap_or_else(|_| panic!("invalid address: {s}"))
}

fn make_node_addr(addr: &str) -> NodeAddr {
    NodeAddr {
        protocol: "quic".into(),
        address: addr.to_string(),
    }
}

/// Handle an incoming request frame, dispatching to the appropriate DhtNode handler.
async fn dispatch_request(
    dht: &Arc<Mutex<DhtNode>>,
    frame: Frame,
    sender: mesh_transport::ResponseSender,
    peer_addr: SocketAddr,
) {
    let response = {
        let mut node = dht.lock().await;
        match frame.msg_type {
            MSG_PING => match from_cbor::<Ping>(&frame.body) {
                Ok(ping) => {
                    let pong = node.handle_ping(&ping);
                    let body = to_cbor(&pong).expect("cbor encode pong");
                    Some(Frame::response(&frame, MSG_PONG, body))
                }
                Err(e) => {
                    error!("bad PING from {peer_addr}: {e}");
                    None
                }
            },
            MSG_STORE => match from_cbor::<Store>(&frame.body) {
                Ok(store_req) => {
                    let ack = node.handle_store(&store_req);
                    let body = to_cbor(&ack).expect("cbor encode store_ack");
                    Some(Frame::response(&frame, MSG_STORE_ACK, body))
                }
                Err(e) => {
                    error!("bad STORE from {peer_addr}: {e}");
                    None
                }
            },
            MSG_FIND_NODE => match from_cbor::<FindNode>(&frame.body) {
                Ok(find_node) => {
                    let result = node.handle_find_node(&find_node);
                    let body = to_cbor(&result).expect("cbor encode find_node_result");
                    Some(Frame::response(&frame, MSG_FIND_NODE_RESULT, body))
                }
                Err(e) => {
                    error!("bad FIND_NODE from {peer_addr}: {e}");
                    None
                }
            },
            MSG_FIND_VALUE => match from_cbor::<FindValue>(&frame.body) {
                Ok(find_value) => {
                    let result = node.handle_find_value(&find_value);
                    let body = to_cbor(&result).expect("cbor encode find_value_result");
                    Some(Frame::response(&frame, MSG_FIND_VALUE_RESULT, body))
                }
                Err(e) => {
                    error!("bad FIND_VALUE from {peer_addr}: {e}");
                    None
                }
            },
            other => {
                error!("unknown msg_type 0x{other:02x} from {peer_addr}");
                None
            }
        }
    };

    if let Some(resp) = response
        && let Err(e) = sender.send(&resp).await
    {
        error!("failed to send response to {peer_addr}: {e}");
    }
}

/// Start a mesh node: bind, bootstrap, then listen.
async fn cmd_start(
    listen: &str,
    seeds: &[String],
    identity_path: Option<&PathBuf>,
) -> anyhow::Result<()> {
    let keypair = load_or_generate_keypair(identity_path);
    let did = keypair.identity().did();
    let socket_addr = parse_addr(listen);

    let endpoint = MeshEndpoint::new(socket_addr)?;
    let local_addr = endpoint.local_addr()?;
    let node_addr = make_node_addr(&local_addr.to_string());

    let dht = Arc::new(Mutex::new(DhtNode::new(
        keypair,
        node_addr,
        DhtConfig::default(),
    )));

    println!("Node started");
    println!("  DID:     {did}");
    println!("  Listen:  {local_addr}");

    // Bootstrap from seeds
    if !seeds.is_empty() {
        let transport =
            QuicTransport::new(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)))?;
        let seed_addrs: Vec<NodeAddr> = seeds.iter().map(|s| make_node_addr(s)).collect();
        let mut node = dht.lock().await;
        match node.bootstrap(&seed_addrs, &transport).await {
            Ok(n) => println!("  Bootstrap: discovered {n} nodes"),
            Err(e) => eprintln!("  Bootstrap failed: {e}"),
        }
    }

    // Listen for incoming connections
    let dht_clone = dht.clone();
    endpoint
        .listen(move |frame, sender| {
            let dht = dht_clone.clone();
            async move {
                // We don't have the peer address readily available in the handler,
                // so we use a placeholder for logging.
                dispatch_request(
                    &dht,
                    frame,
                    sender,
                    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)),
                )
                .await;
            }
        })
        .await?;

    Ok(())
}

/// Publish a capability descriptor.
async fn cmd_publish(
    cap_type: &str,
    endpoint_addr: &str,
    params: Option<&str>,
    seed: &str,
    identity_path: Option<&PathBuf>,
) -> anyhow::Result<()> {
    let keypair = load_or_generate_keypair(identity_path);
    let did = keypair.identity().did();

    // Build capability payload as JSON
    let payload = serde_json::json!({
        "type": cap_type,
        "endpoint": endpoint_addr,
        "params": params.map(|p| serde_json::from_str::<serde_json::Value>(p).unwrap_or(serde_json::Value::String(p.to_string()))),
    });
    let payload_bytes = serde_json::to_vec(&payload)?;

    // Compute routing keys (hierarchical)
    let rkeys = hierarchical_routing_keys(cap_type);

    let descriptor = Descriptor::create(
        &keypair,
        schema_hash("core/capability"),
        cap_type.to_string(),
        payload_bytes,
        now_micros(),
        1,
        3600,
        rkeys,
    )?;

    println!("Publishing capability:");
    println!("  DID:    {did}");
    println!("  Type:   {cap_type}");
    println!("  ID:     {}", descriptor.id);

    // Connect to seed and STORE
    let transport = QuicTransport::new(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)))?;
    let seed_addr = make_node_addr(seed);

    let store = Store {
        sender: keypair.identity(),
        descriptor,
    };
    let body = to_cbor(&store).expect("cbor encode store");
    let frame = Frame::new(MSG_STORE, body);

    let resp = mesh_dht::transport::Transport::send_request(&transport, &seed_addr, frame)
        .await
        .map_err(|e| anyhow::anyhow!("transport error: {e}"))?;

    if resp.msg_type == MSG_STORE_ACK {
        let ack: StoreAck = from_cbor(&resp.body)?;
        if ack.stored {
            println!("  Result:  stored successfully");
        } else {
            println!("  Result:  rejected — {}", ack.reason.unwrap_or_default());
        }
    } else {
        println!(
            "  Result:  unexpected response type 0x{:02x}",
            resp.msg_type
        );
    }

    Ok(())
}

/// Discover capabilities on the mesh.
async fn cmd_discover(
    cap_type: &str,
    seed: &str,
    identity_path: Option<&PathBuf>,
) -> anyhow::Result<()> {
    let keypair = load_or_generate_keypair(identity_path);
    let rk = routing_key(cap_type);

    println!("Discovering: {cap_type}");
    println!("  Routing key: {rk}");

    let transport = QuicTransport::new(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)))?;

    // First add seed to routing table so lookup_value can find it
    let seed_addr = make_node_addr(seed);

    // Send a PING to get the seed's identity
    let ping = Ping {
        sender: keypair.identity(),
        sender_addr: NodeAddr {
            protocol: "quic".into(),
            address: "0.0.0.0:0".into(),
        },
    };
    let ping_body = to_cbor(&ping).expect("cbor encode ping");
    let ping_frame = Frame::new(MSG_PING, ping_body);

    let pong_frame =
        mesh_dht::transport::Transport::send_request(&transport, &seed_addr, ping_frame)
            .await
            .map_err(|e| anyhow::anyhow!("transport error: {e}"))?;
    let _pong: Pong = from_cbor(&pong_frame.body)?;

    // Now do FIND_VALUE directly
    let find_value = FindValue {
        sender: keypair.identity(),
        key: rk,
        max_results: 20,
        filters: None,
    };
    let body = to_cbor(&find_value).expect("cbor encode find_value");
    let frame = Frame::new(MSG_FIND_VALUE, body);

    let resp = mesh_dht::transport::Transport::send_request(&transport, &seed_addr, frame)
        .await
        .map_err(|e| anyhow::anyhow!("transport error: {e}"))?;

    if resp.msg_type == MSG_FIND_VALUE_RESULT {
        let result: FindValueResult = from_cbor(&resp.body)?;
        if let Some(descriptors) = result.descriptors {
            println!("  Found {} descriptor(s):", descriptors.len());
            for desc in &descriptors {
                println!("    ---");
                println!("    Publisher: {}", desc.publisher.did());
                println!("    Topic:     {}", desc.topic);
                println!("    ID:        {}", desc.id);
                // Try to parse payload as JSON for pretty printing
                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&desc.payload) {
                    println!("    Payload:   {val}");
                }
            }
        } else if let Some(nodes) = result.nodes {
            println!("  No descriptors found; got {} closer node(s)", nodes.len());
        } else {
            println!("  Empty response");
        }
    } else {
        println!("  Unexpected response type 0x{:02x}", resp.msg_type);
    }

    Ok(())
}

/// Ping a remote node.
async fn cmd_ping(addr: &str, identity_path: Option<&PathBuf>) -> anyhow::Result<()> {
    let keypair = load_or_generate_keypair(identity_path);
    let transport = QuicTransport::new(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)))?;

    let target_addr = make_node_addr(addr);
    let ping = Ping {
        sender: keypair.identity(),
        sender_addr: NodeAddr {
            protocol: "quic".into(),
            address: "0.0.0.0:0".into(),
        },
    };
    let body = to_cbor(&ping).expect("cbor encode ping");
    let frame = Frame::new(MSG_PING, body);

    let resp = mesh_dht::transport::Transport::send_request(&transport, &target_addr, frame)
        .await
        .map_err(|e| anyhow::anyhow!("transport error: {e}"))?;

    if resp.msg_type == MSG_PONG {
        let pong: Pong = from_cbor(&resp.body)?;
        println!("PONG from {}", pong.sender.did());
        println!(
            "  Observed addr: {}://{}",
            pong.observed_addr.protocol, pong.observed_addr.address
        );
        println!(
            "  Sender addr:   {}://{}",
            pong.sender_addr.protocol, pong.sender_addr.address
        );
    } else {
        println!("Unexpected response type 0x{:02x}", resp.msg_type);
    }

    Ok(())
}

/// Identity management.
fn cmd_identity(generate: bool, path: Option<&PathBuf>) {
    if generate {
        let kp = Keypair::generate();
        println!("Generated new identity:");
        println!("  DID: {}", kp.identity().did());
        if let Some(p) = path {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).expect("failed to create directory");
            }
            std::fs::write(p, kp.secret_bytes()).expect("failed to write key file");
            println!("  Saved to: {}", p.display());
        } else {
            println!("  Secret (hex): {}", hex::encode(kp.secret_bytes()));
        }
    } else if let Some(p) = path {
        if p.exists() {
            let bytes = std::fs::read(p).expect("failed to read key file");
            let secret: [u8; 32] = bytes.try_into().expect("key file must be exactly 32 bytes");
            let kp = Keypair::from_bytes(&secret);
            println!("Identity from {}:", p.display());
            println!("  DID: {}", kp.identity().did());
        } else {
            eprintln!("Key file not found: {}", p.display());
            std::process::exit(1);
        }
    } else {
        eprintln!("Specify --generate to create a new identity, or --path to load one.");
        std::process::exit(1);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match &cli.command {
        Commands::Start {
            listen,
            seed,
            identity,
        } => cmd_start(listen, seed, identity.as_ref()).await?,
        Commands::Publish {
            cap_type,
            endpoint,
            params,
            seed,
            identity,
        } => {
            cmd_publish(
                cap_type,
                endpoint,
                params.as_deref(),
                seed,
                identity.as_ref(),
            )
            .await?;
        }
        Commands::Discover {
            cap_type,
            seed,
            identity,
        } => cmd_discover(cap_type, seed, identity.as_ref()).await?,
        Commands::Ping { addr, identity } => cmd_ping(addr, identity.as_ref()).await?,
        Commands::Identity { generate, path } => cmd_identity(*generate, path.as_ref()),
    }

    Ok(())
}
