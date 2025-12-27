//! benchScale CLI

use benchscale::{init, Backend, Config, DockerBackend, Lab, LabRegistry, Topology};
use tracing::{error, info};

#[tokio::main]
async fn main() {
    init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let command = &args[1];

    match command.as_str() {
        "create" => {
            if args.len() < 4 {
                eprintln!("Usage: benchscale create <lab-name> <topology-file>");
                std::process::exit(1);
            }
            let lab_name = &args[2];
            let topology_file = &args[3];

            if let Err(e) = create_lab(lab_name, topology_file).await {
                error!("Failed to create lab: {}", e);
                std::process::exit(1);
            }
        }
        "destroy" => {
            if args.len() < 3 {
                eprintln!("Usage: benchscale destroy <lab-name>");
                std::process::exit(1);
            }
            let lab_name = &args[2];

            if let Err(e) = destroy_lab(lab_name).await {
                error!("Failed to destroy lab: {}", e);
                std::process::exit(1);
            }
        }
        "list" => {
            if let Err(e) = list_labs().await {
                error!("Failed to list labs: {}", e);
                std::process::exit(1);
            }
        }
        "status" => {
            if args.len() < 3 {
                eprintln!("Usage: benchscale status <lab-name>");
                std::process::exit(1);
            }
            let lab_name = &args[2];

            if let Err(e) = show_status(lab_name).await {
                error!("Failed to get status: {}", e);
                std::process::exit(1);
            }
        }
        "version" | "--version" | "-v" => {
            println!("benchScale v{}", benchscale::VERSION);
        }
        "help" | "--help" | "-h" => {
            print_usage();
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            print_usage();
            std::process::exit(1);
        }
    }
}

async fn create_lab(lab_name: &str, topology_file: &str) -> anyhow::Result<()> {
    info!(
        "Creating lab '{}' from topology '{}'",
        lab_name, topology_file
    );

    // Load configuration
    let config = Config::from_env();

    // Load topology
    let topology = Topology::from_file(topology_file).await?;
    info!("Loaded topology: {}", topology.metadata.name);

    // Create Docker backend
    let backend = DockerBackend::new()?;

    // Check if Docker is available
    if !backend.is_available().await? {
        anyhow::bail!("Docker is not available. Please ensure Docker is installed and running.");
    }

    // Create lab
    let lab = Lab::create(lab_name, topology.clone(), backend).await?;

    // Register lab in registry
    let registry = LabRegistry::from_config(&config);
    registry
        .register_lab(
            lab.id().to_string(),
            lab_name.to_string(),
            topology,
            "docker".to_string(),
        )
        .await?;

    info!("Lab '{}' created successfully!", lab_name);
    info!("Lab ID: {}", lab.id());
    info!("Nodes:");
    for node in lab.nodes().await {
        info!("  - {} ({}): {:?}", node.name, node.ip_address, node.status);
    }

    Ok(())
}

async fn destroy_lab(lab_name: &str) -> anyhow::Result<()> {
    info!("Destroying lab '{}'", lab_name);

    // Load configuration
    let config = Config::from_env();
    let registry = LabRegistry::from_config(&config);

    // Load lab metadata
    let metadata = registry.load_lab_by_name(lab_name).await?;

    info!("Found lab: {} (ID: {})", metadata.name, metadata.id);

    // Recreate backend (we don't have the Lab object anymore)
    let backend = DockerBackend::new()?;

    // Delete all nodes
    for node_id in &metadata.node_ids {
        info!("Deleting node: {}", node_id);
        if let Err(e) = backend.delete_node(node_id).await {
            error!("Failed to delete node {}: {}", node_id, e);
        }
    }

    // Delete network
    if let Some(network_id) = &metadata.network_id {
        info!("Deleting network: {}", network_id);
        if let Err(e) = backend
            .delete_network(&metadata.topology.network.name)
            .await
        {
            error!("Failed to delete network: {}", e);
        }
    }

    // Remove from registry
    registry.delete_lab(&metadata.id).await?;

    info!("Lab '{}' destroyed successfully", lab_name);

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
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    for lab in labs {
        let status_icon = match lab.status {
            benchscale::LabStatus::Creating => "🔄",
            benchscale::LabStatus::Running => "✅",
            benchscale::LabStatus::Destroying => "⏳",
            benchscale::LabStatus::Destroyed => "💀",
            benchscale::LabStatus::Failed => "❌",
        };

        println!(
            "{} {} ({})",
            status_icon,
            lab.name,
            format!("{:?}", lab.status).to_lowercase()
        );
        println!("   ID: {}", lab.id);
        println!("   Backend: {}", lab.backend_type);
        println!("   Nodes: {}", lab.node_ids.len());
        println!("   Created: {}", lab.created_at.format("%Y-%m-%d %H:%M:%S"));
        println!();
    }

    Ok(())
}

async fn show_status(lab_name: &str) -> anyhow::Result<()> {
    let config = Config::from_env();
    let registry = LabRegistry::from_config(&config);

    let metadata = registry.load_lab_by_name(lab_name).await?;

    println!("\nLab Status: {}", metadata.name);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("ID: {}", metadata.id);
    println!("Status: {:?}", metadata.status);
    println!("Backend: {}", metadata.backend_type);
    println!("Topology: {}", metadata.topology.metadata.name);
    println!("Network: {}", metadata.topology.network.name);
    println!("Nodes: {}", metadata.node_ids.len());

    if !metadata.node_ids.is_empty() {
        println!("\nNodes:");
        for node_id in &metadata.node_ids {
            println!("  - {}", node_id);
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

fn print_usage() {
    println!(
        r#"
benchScale v{} - Pure Rust Laboratory Substrate

USAGE:
    benchscale <COMMAND> [OPTIONS]

COMMANDS:
    create <name> <topology>    Create a new lab from a topology file
    destroy <name>              Destroy an existing lab
    list                        List all active labs
    version                     Show version information
    help                        Show this help message

EXAMPLES:
    # Create a lab from a topology file
    benchscale create my-lab topologies/simple-lan.yaml
    
    # Destroy a lab
    benchscale destroy my-lab
    
    # List all labs
    benchscale list

For more information, visit: https://github.com/ecoPrimals/benchScale
"#,
        benchscale::VERSION
    );
}
