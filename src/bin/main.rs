//! benchScale CLI

use benchscale::{Lab, DockerBackend, Topology, init};
use std::path::PathBuf;
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
            let _lab_name = &args[2];
            eprintln!("Destroy command not yet implemented (requires lab persistence)");
            std::process::exit(1);
        }
        "list" => {
            eprintln!("List command not yet implemented (requires lab persistence)");
            std::process::exit(1);
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
    info!("Creating lab '{}' from topology '{}'", lab_name, topology_file);

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
    let lab = Lab::create(lab_name, topology, backend).await?;
    
    info!("Lab '{}' created successfully!", lab_name);
    info!("Lab ID: {}", lab.id());
    info!("Nodes:");
    for node in lab.nodes().await {
        info!("  - {} ({}): {}", node.name, node.ip_address, node.status);
    }

    Ok(())
}

fn print_usage() {
    println!(r#"
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
"#, benchscale::VERSION);
}

