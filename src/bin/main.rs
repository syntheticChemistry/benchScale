// SPDX-License-Identifier: AGPL-3.0-only
//! benchScale CLI — typed argument parsing via clap
#![allow(deprecated)] // CLI still uses legacy `Config` until migrated to BenchScaleConfig

use benchscale::{init, Backend, Config, DockerBackend, Lab, LabRegistry, Topology};
use clap::{Parser, Subcommand};
use tracing::{error, info};

#[derive(Parser)]
#[command(
    name = "benchscale",
    about = "Pure Rust laboratory substrate for distributed system testing",
    version = benchscale::VERSION,
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new lab from a topology file
    Create {
        /// Lab name
        name: String,
        /// Path to topology YAML file
        topology: String,
        /// Backend to use
        #[arg(long, default_value = "docker")]
        backend: String,
    },
    /// Destroy an existing lab
    Destroy {
        /// Lab name
        name: String,
        /// Force destroy without confirmation
        #[arg(long)]
        force: bool,
    },
    /// List all active labs
    List,
    /// Show detailed status of a lab
    Status {
        /// Lab name
        name: String,
    },
    /// Start JSON-RPC 2.0 server (UniBin compliance)
    Server {
        /// TCP port to listen on
        #[arg(long)]
        port: u16,
        /// Bind address
        #[arg(long, default_value = "127.0.0.1")]
        listen: String,
        /// Run in standalone mode (no Songbird registration)
        #[arg(long, default_value_t = true)]
        standalone: bool,
    },
    /// Validate IPC compliance of a running primal endpoint
    Validate {
        #[command(subcommand)]
        target: ValidateTarget,
    },
    /// Show version information
    Version,
}

/// Validation sub-commands
#[derive(Subcommand)]
enum ValidateTarget {
    /// Check JSON-RPC health compliance at a TCP endpoint
    Ipc {
        /// Endpoint address (host:port)
        endpoint: String,
    },
}

#[tokio::main]
async fn main() {
    init();
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Create {
            name,
            topology,
            backend: _,
        } => create_lab(&name, &topology).await,
        Command::Destroy { name, force: _ } => destroy_lab(&name).await,
        Command::List => list_labs().await,
        Command::Status { name } => show_status(&name).await,
        Command::Server {
            port,
            listen,
            standalone,
        } => {
            let addr: std::net::SocketAddr = format!("{listen}:{port}")
                .parse()
                .expect("invalid listen address");
            benchscale::server::run_server(addr, standalone).await
        }
        Command::Validate { target } => match target {
            ValidateTarget::Ipc { endpoint } => validate_ipc(&endpoint).await,
        },
        Command::Version => {
            println!("benchScale v{}", benchscale::VERSION);
            Ok(())
        }
    };

    if let Err(e) = result {
        error!("{e}");
        std::process::exit(1);
    }
}

async fn create_lab(lab_name: &str, topology_file: &str) -> anyhow::Result<()> {
    info!("Creating lab '{lab_name}' from topology '{topology_file}'");

    let config = Config::from_env();
    let topology = Topology::from_file(topology_file).await?;
    info!("Loaded topology: {}", topology.metadata.name);

    let backend = DockerBackend::new()?;
    if !backend.is_available().await? {
        anyhow::bail!("Docker is not available. Ensure Docker is installed and running.");
    }

    let lab = Lab::create(lab_name, topology.clone(), backend).await?;

    let registry = LabRegistry::from_config(&config);
    registry
        .register_lab(
            lab.id().to_string(),
            lab_name.to_string(),
            topology,
            "docker".to_string(),
        )
        .await?;

    info!("Lab '{lab_name}' created — ID: {}", lab.id());
    for node in lab.nodes().await {
        info!("  {} ({}): {:?}", node.name, node.ip_address, node.status);
    }

    Ok(())
}

async fn destroy_lab(lab_name: &str) -> anyhow::Result<()> {
    info!("Destroying lab '{lab_name}'");

    let config = Config::from_env();
    let registry = LabRegistry::from_config(&config);
    let metadata = registry.load_lab_by_name(lab_name).await?;

    info!("Found lab: {} (ID: {})", metadata.name, metadata.id);

    let backend = DockerBackend::new()?;

    for node_id in &metadata.node_ids {
        info!("Deleting node: {node_id}");
        if let Err(e) = backend.delete_node(node_id).await {
            error!("Failed to delete node {node_id}: {e}");
        }
    }

    if let Some(network_id) = &metadata.network_id {
        info!("Deleting network: {network_id}");
        if let Err(e) = backend
            .delete_network(&metadata.topology.network.name)
            .await
        {
            error!("Failed to delete network: {e}");
        }
    }

    registry.delete_lab(&metadata.id).await?;
    info!("Lab '{lab_name}' destroyed");

    Ok(())
}

async fn list_labs() -> anyhow::Result<()> {
    let config = Config::from_env();
    let registry = LabRegistry::from_config(&config);
    let labs = registry.list_labs().await?;

    if labs.is_empty() {
        println!("No labs found.");
        return Ok(());
    }

    println!("\nActive Labs:");
    println!("{}", "━".repeat(54));

    for lab in labs {
        let status_str = format!("{:?}", lab.status).to_lowercase();
        println!("{} ({})", lab.name, status_str);
        println!("   ID: {}", lab.id);
        println!("   Backend: {}", lab.backend_type);
        println!("   Nodes: {}", lab.node_ids.len());
        println!(
            "   Created: {}",
            lab.created_at.format("%Y-%m-%d %H:%M:%S")
        );
        println!();
    }

    Ok(())
}

async fn show_status(lab_name: &str) -> anyhow::Result<()> {
    let config = Config::from_env();
    let registry = LabRegistry::from_config(&config);
    let metadata = registry.load_lab_by_name(lab_name).await?;

    println!("\nLab: {}", metadata.name);
    println!("{}", "━".repeat(54));
    println!("ID:       {}", metadata.id);
    println!("Status:   {:?}", metadata.status);
    println!("Backend:  {}", metadata.backend_type);
    println!("Topology: {}", metadata.topology.metadata.name);
    println!("Network:  {}", metadata.topology.network.name);
    println!("Nodes:    {}", metadata.node_ids.len());

    if !metadata.node_ids.is_empty() {
        println!("\nNodes:");
        for node_id in &metadata.node_ids {
            println!("  - {node_id}");
        }
    }

    println!(
        "\nCreated: {}",
        metadata.created_at.format("%Y-%m-%d %H:%M:%S")
    );
    println!(
        "Updated: {}",
        metadata.updated_at.format("%Y-%m-%d %H:%M:%S")
    );

    Ok(())
}

async fn validate_ipc(endpoint: &str) -> anyhow::Result<()> {
    let addr: std::net::SocketAddr = endpoint
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid endpoint '{endpoint}': {e}"))?;

    info!("Validating IPC compliance at {addr}");

    let validator = benchscale::validation::IpcComplianceValidator::new();
    let report = validator.validate(addr).await;

    println!("\nIPC Compliance Report: {}", report.endpoint);
    println!("{}", "━".repeat(54));

    for result in &report.results {
        let status = if result.passed { "✓" } else { "✗" };
        println!(
            "  {status} {} ({}ms)",
            result.method, result.response_ms
        );
        if let Some(ref err) = result.error {
            println!("    → {err}");
        }
    }

    println!();
    if report.compliant {
        println!("Result: COMPLIANT");
    } else {
        println!("Result: NON-COMPLIANT");
    }

    if !report.compliant {
        anyhow::bail!("endpoint {addr} is not IPC-compliant");
    }

    Ok(())
}
