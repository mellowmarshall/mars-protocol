//! Publish a capability to a mesh node, then discover it.
//!
//! Usage:
//!   cargo run --example publish_and_discover -- --seed 127.0.0.1:4433

use std::net::SocketAddr;

use clap::Parser;
use mesh_client::{
    schema_hash, routing_key, Keypair, MeshClient, NodeAddr,
};

#[derive(Parser)]
#[command(name = "publish-and-discover")]
#[command(about = "Publish a capability and discover it on the mesh")]
struct Args {
    /// Seed node address (e.g., 127.0.0.1:4433)
    #[arg(long)]
    seed: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let seed = NodeAddr::quic(&args.seed);

    // 1. Generate a fresh identity
    let keypair = Keypair::generate();
    println!("Identity: {}", keypair.identity().did());

    // 2. Create a client bound to an ephemeral port
    let bind: SocketAddr = "127.0.0.1:0".parse()?;
    let mut client = MeshClient::new(keypair, bind).await?;
    println!("Listening: {}", client.local_addr()?);

    // 3. Bootstrap
    let discovered = client.bootstrap(&[seed.clone()]).await?;
    println!("Bootstrap: {discovered} node(s) discovered");

    // 4. Publish a capability
    let cap_type = "compute/inference/text-generation";
    let endpoint = "https://api.example.com/v1/generate";
    let ack = client
        .publish_capability(cap_type, endpoint, None, &seed)
        .await?;
    println!("Published: stored={}", ack.stored);

    // 5. Discover capabilities at the same routing key
    let key = routing_key(cap_type);
    let descriptors = client.discover(&key).await?;
    println!("Discovered: {} descriptor(s)", descriptors.len());
    for desc in &descriptors {
        let matches_schema = desc.schema_hash == schema_hash("core/capability");
        println!(
            "  id={} topic={} schema_match={}",
            desc.id.to_hex(),
            desc.topic,
            matches_schema,
        );
    }

    Ok(())
}
