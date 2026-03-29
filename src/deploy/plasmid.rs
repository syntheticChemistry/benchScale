// SPDX-License-Identifier: AGPL-3.0-only
//! plasmidBin binary resolution for lab deployment.
//!
//! Given a `plasmidBin` directory, resolves primal binaries by
//! target architecture and deploys them to running lab nodes.

use std::path::{Path, PathBuf};

use tracing::info;

use super::arch::{Arch, BinaryResolver};
use crate::{Backend, Result};

/// Deploy primals from a `plasmidBin` directory into a running lab node.
///
/// Resolves binaries for the given architecture and copies them to
/// `/opt/biomeos/bin/` inside the container.
pub async fn deploy_primals_to_node(
    backend: &dyn Backend,
    node_id: &str,
    plasmid_bin_path: &Path,
    arch: Arch,
    primal_names: &[&str],
) -> Result<Vec<DeployedBinary>> {
    let resolver = BinaryResolver::new(plasmid_bin_path, arch);
    let mut deployed = Vec::new();

    backend
        .exec_command(
            node_id,
            vec!["mkdir".into(), "-p".into(), "/opt/biomeos/bin".into()],
        )
        .await?;

    for name in primal_names {
        match resolver.resolve(name) {
            Ok(local_path) => {
                let dest = format!("/opt/biomeos/bin/{name}");
                info!("deploying {name} ({arch}) to {node_id}:{dest}");

                backend
                    .copy_to_node(node_id, local_path.to_str().unwrap_or(""), &dest)
                    .await?;

                backend
                    .exec_command(node_id, vec!["chmod".into(), "+x".into(), dest.clone()])
                    .await?;

                deployed.push(DeployedBinary {
                    name: (*name).to_string(),
                    local_path,
                    remote_path: dest,
                });
            }
            Err(e) => {
                info!("skipping {name}: {e}");
            }
        }
    }

    Ok(deployed)
}

/// Record of a binary deployed to a lab node.
#[derive(Debug, Clone)]
pub struct DeployedBinary {
    /// Primal binary name.
    pub name: String,
    /// Source path on the host.
    pub local_path: PathBuf,
    /// Destination path inside the container/VM.
    pub remote_path: String,
}

/// List all available primal binaries in a `plasmidBin` directory
/// for a given architecture.
pub fn list_available_primals(plasmid_bin_path: &Path, arch: Arch) -> Vec<String> {
    BinaryResolver::new(plasmid_bin_path, arch).list_available()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_list_available_primals_empty() {
        let tmp = TempDir::new().expect("tmpdir");
        let primals = list_available_primals(tmp.path(), Arch::X86_64);
        assert!(primals.is_empty());
    }

    #[test]
    fn test_list_available_primals_populated() {
        let tmp = TempDir::new().expect("tmpdir");
        let arch_dir = tmp.path().join("primals").join("x86_64");
        std::fs::create_dir_all(&arch_dir).expect("mkdir");
        std::fs::write(arch_dir.join("beardog"), b"ELF").expect("write");
        std::fs::write(arch_dir.join("songbird"), b"ELF").expect("write");
        std::fs::write(arch_dir.join("nestgate"), b"ELF").expect("write");

        let primals = list_available_primals(tmp.path(), Arch::X86_64);
        assert_eq!(primals.len(), 3);
    }
}
