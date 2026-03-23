//! `mesh-hub` CLI binary — thin wrapper over [`HubRuntime`].

use std::path::PathBuf;

use clap::Parser;
use mesh_core::identity::Keypair;
use mesh_hub::HubRuntime;
use mesh_hub::config::HubConfig;

#[derive(Parser)]
#[command(name = "mesh-hub", about = "Mesh Protocol Hub Node")]
struct Cli {
    /// Path to hub configuration file (TOML)
    #[arg(short, long, default_value = "mesh-hub.toml")]
    config: PathBuf,

    /// Generate a new keypair and write to the configured path
    #[arg(long)]
    generate_keypair: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let config = HubConfig::from_file(&cli.config)?;

    if cli.generate_keypair {
        generate_keypair(&config.identity.keypair_path)?;
        return Ok(());
    }

    let keypair = load_keypair(&config.identity.keypair_path)?;
    tracing::info!(did = %keypair.identity().did(), "loaded hub identity");

    let runtime = HubRuntime::builder(config, keypair).build()?;
    runtime.run().await?;

    Ok(())
}

fn load_keypair(path: &std::path::Path) -> Result<Keypair, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path).map_err(|e| format!("failed to read keypair at {}: {e}", path.display()))?;
    if bytes.len() != 32 {
        return Err(format!(
            "keypair file must be exactly 32 bytes (got {})",
            bytes.len()
        )
        .into());
    }
    let secret: [u8; 32] = bytes.try_into().unwrap();
    Ok(Keypair::from_bytes(&secret))
}

fn generate_keypair(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        return Err(format!("keypair file already exists: {}", path.display()).into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let kp = Keypair::generate();

    // Write with restrictive permissions
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(&kp.secret_bytes())?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, &kp.secret_bytes())?;
    }

    println!("Generated keypair at {}", path.display());
    println!("  DID: {}", kp.identity().did());
    Ok(())
}
