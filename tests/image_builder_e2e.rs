// SPDX-License-Identifier: AGPL-3.0-or-later
//! E2E tests for ImageBuilder
//!
//! Tests the complete image building workflow with real VMs.

#![cfg(all(test, feature = "libvirt"))]

use benchscale::{BuildStep, CloudInit, ImageBuilder};
use std::path::PathBuf;

#[tokio::test]
#[ignore] // Requires libvirt
async fn test_basic_image_build() -> anyhow::Result<()> {
    // This tests the basic workflow:
    // 1. Create builder VM
    // 2. Wait for cloud-init
    // 3. Save as template

    let base_image =
        PathBuf::from("../agentReagents/images/cloud/ubuntu-24.04-server-cloudimg-amd64.img");

    if !base_image.exists() {
        eprintln!("Skipping test - base image not found");
        return Ok(());
    }

    let builder = ImageBuilder::new_libvirt("test-basic-image")?
        .from_cloud_image(base_image)
        .with_memory(2048)
        .with_vcpus(1)
        .add_step(BuildStep::WaitForCloudInit);

    let result = builder.build().await?;

    assert!(result.template_path.exists());
    assert!(result.final_size_bytes > 0);

    // Cleanup
    std::fs::remove_file(result.template_path)?;

    Ok(())
}

#[tokio::test]
#[ignore] // Requires libvirt
async fn test_image_build_with_packages() -> anyhow::Result<()> {
    // This tests:
    // 1. Create builder VM
    // 2. Wait for cloud-init (handles apt locks!)
    // 3. Install packages
    // 4. Save as template

    let base_image =
        PathBuf::from("../agentReagents/images/cloud/ubuntu-24.04-server-cloudimg-amd64.img");

    if !base_image.exists() {
        eprintln!("Skipping test - base image not found");
        return Ok(());
    }

    let builder = ImageBuilder::new_libvirt("test-with-packages")?
        .from_cloud_image(base_image)
        .with_memory(2048)
        .with_vcpus(1)
        .add_step(BuildStep::WaitForCloudInit) // Critical: wait for apt lock!
        .add_step(BuildStep::InstallPackages(vec![
            "vim".to_string(),
            "curl".to_string(),
        ]));

    let result = builder.build().await?;

    assert!(result.template_path.exists());

    // Cleanup
    std::fs::remove_file(result.template_path)?;

    Ok(())
}

#[tokio::test]
#[ignore] // Requires libvirt and user interaction
async fn test_image_build_with_user_verification() -> anyhow::Result<()> {
    // This tests the user interaction workflow:
    // 1. Create builder VM
    // 2. Install something visual
    // 3. Pause for user to verify via VNC
    // 4. Save as template

    let base_image =
        PathBuf::from("../agentReagents/images/cloud/ubuntu-24.04-server-cloudimg-amd64.img");

    if !base_image.exists() {
        eprintln!("Skipping test - base image not found");
        return Ok(());
    }

    let builder = ImageBuilder::new_libvirt("test-user-verify")?
        .from_cloud_image(base_image)
        .with_memory(4096)
        .with_vcpus(2)
        .add_step(BuildStep::WaitForCloudInit)
        .add_step(BuildStep::InstallPackages(vec![
            "ubuntu-desktop".to_string(),
        ]))
        .add_step(BuildStep::Reboot)
        .add_step(BuildStep::UserVerification {
            message: "Check VNC - is Ubuntu desktop running?".to_string(),
            vnc_port: None,
        });

    let result = builder.build().await?;

    assert!(result.template_path.exists());

    // Cleanup
    std::fs::remove_file(result.template_path)?;

    Ok(())
}

#[tokio::test]
#[ignore] // Requires libvirt
async fn test_intermediate_save() -> anyhow::Result<()> {
    // This tests intermediate state saving:
    // 1. Create builder VM
    // 2. Install something
    // 3. Save intermediate state
    // 4. Install more
    // 5. Save final template

    let base_image =
        PathBuf::from("../agentReagents/images/cloud/ubuntu-24.04-server-cloudimg-amd64.img");

    if !base_image.exists() {
        eprintln!("Skipping test - base image not found");
        return Ok(());
    }

    let intermediate_path = PathBuf::from("/tmp/test-intermediate.qcow2");

    let builder = ImageBuilder::new_libvirt("test-intermediate")?
        .from_cloud_image(base_image)
        .with_memory(2048)
        .with_vcpus(1)
        .add_step(BuildStep::WaitForCloudInit)
        .add_step(BuildStep::InstallPackages(vec!["vim".to_string()]))
        .add_step(BuildStep::SaveIntermediate {
            name: "after-vim".to_string(),
            path: intermediate_path.clone(),
        })
        .add_step(BuildStep::InstallPackages(vec!["curl".to_string()]));

    let result = builder.build().await?;

    assert!(result.template_path.exists());
    assert!(intermediate_path.exists());

    // Cleanup
    std::fs::remove_file(result.template_path)?;
    std::fs::remove_file(intermediate_path)?;

    Ok(())
}

#[tokio::test]
#[ignore] // Requires libvirt
async fn test_apt_lock_handling() -> anyhow::Result<()> {
    // This specifically tests that we wait for cloud-init/apt locks
    // This was the bug we found - cloud-init was holding apt lock
    // and package installation failed

    let base_image =
        PathBuf::from("../agentReagents/images/cloud/ubuntu-24.04-server-cloudimg-amd64.img");

    if !base_image.exists() {
        eprintln!("Skipping test - base image not found");
        return Ok(());
    }

    let cloud_init = CloudInit::builder()
        .add_user("builder", "") // Empty SSH key (will use password)
        // Add packages in cloud-init to create apt lock situation
        .package("htop")
        .package("tree")
        .build();

    let builder = ImageBuilder::new_libvirt("test-apt-lock")?
        .from_cloud_image(base_image)
        .with_memory(2048)
        .with_vcpus(1)
        .with_cloud_init(cloud_init)
        .add_step(BuildStep::WaitForCloudInit) // This should wait for cloud-init apt to finish!
        .add_step(BuildStep::InstallPackages(vec!["vim".to_string()])); // This should work now

    let result = builder.build().await?;

    assert!(result.template_path.exists());

    // Cleanup
    std::fs::remove_file(result.template_path)?;

    Ok(())
}
