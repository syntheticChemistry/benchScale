// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pipeline orchestration for image builds: VM bring-up, sequential steps, template export.
//!
//! Stage types and per-step execution live in [`super::stages`].

use super::stages::{detect_ssh_user, get_actual_vm_ip, wait_for_ssh};
use super::{BuildResult, ImageBuilder, Result};
use crate::Error;
use crate::backend::{NodeInfo, NodeStatus};
use tracing::info;

impl ImageBuilder {
    /// Build from existing VM (simplified workflow based on pipeline lessons)
    pub async fn build_from_existing(self, vm_name: &str) -> Result<BuildResult> {
        info!("Building from existing VM: {}", vm_name);

        info!("Step 1/4: Getting actual VM IP...");
        let ip = get_actual_vm_ip(vm_name).await?;

        info!("Step 2/4: Detecting SSH user...");
        let user = detect_ssh_user(&ip).await?;

        info!("Step 3/4: Waiting for SSH to be ready...");
        wait_for_ssh(&ip, &user, 10).await?;

        info!("Step 4/4: Executing {} build steps...", self.steps.len());

        let node = NodeInfo {
            id: vm_name.to_string(),
            name: vm_name.to_string(),
            container_id: vm_name.to_string(),
            ip_address: ip.clone(),
            network: "default".to_string(),
            status: NodeStatus::Running,
            metadata: std::collections::HashMap::new(),
        };

        for (idx, step) in self.steps.iter().enumerate() {
            info!(
                "  Executing step {}/{}: {:?}",
                idx + 1,
                self.steps.len(),
                step
            );
            self.execute_step_with_user(&node, &user, step).await?;
        }

        info!("Saving VM as template...");
        let template_path = self.save_as_template(vm_name).await?;

        let final_size = std::fs::metadata(&template_path)
            .map(|m| m.len())
            .unwrap_or(0);

        info!("Build complete!");

        Ok(BuildResult {
            template_path,
            vm_name: vm_name.to_string(),
            final_size_bytes: final_size,
        })
    }

    /// Build the template
    pub async fn build(mut self) -> Result<BuildResult> {
        let base_image = self
            .base_image
            .take()
            .ok_or_else(|| Error::Backend("No base image specified".to_string()))?;

        if !base_image.exists() {
            return Err(Error::Backend(format!(
                "Base image not found: {}",
                base_image.display()
            )));
        }

        let vm_name = format!(
            "{}-builder-{}",
            self.name,
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        );

        info!("Starting image build: {}", vm_name);
        info!("  Base image: {}", base_image.display());
        info!("  Memory: {}MB, vCPUs: {}", self.memory_mb, self.vcpus);
        info!("  Build steps: {}", self.steps.len());

        let node = self.create_builder_vm(&vm_name, &base_image).await?;

        info!("Builder VM created: {} at {}", node.name, node.ip_address);

        let vnc_display =
            Self::get_vnc_display(&vm_name).unwrap_or_else(|_| "(unknown)".to_string());
        info!("  VNC: {}", vnc_display);

        for (idx, step) in self.steps.iter().enumerate() {
            info!(
                "Executing step {}/{}: {:?}",
                idx + 1,
                self.steps.len(),
                step
            );
            self.execute_step(&node, step).await?;
        }

        let template_path = self.save_as_template(&vm_name).await?;

        info!("Cleaning up builder VM...");
        self.backend.delete_node(&vm_name).await?;

        let final_size = std::fs::metadata(&template_path)
            .map(|m| m.len())
            .unwrap_or(0);

        Ok(BuildResult {
            template_path,
            vm_name,
            final_size_bytes: final_size,
        })
    }
}
