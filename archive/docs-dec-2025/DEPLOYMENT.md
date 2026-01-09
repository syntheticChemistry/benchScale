# benchScale Deployment Guide

**Version:** 2.0.0  
**Date:** December 27, 2025  
**Status:** Production Ready ✅

---

## 🚀 Quick Start

### Prerequisites

- Docker 20.10+ (for Docker backend)
- Rust 1.70+ (for building from source)
- libvirt/KVM (optional, for VM backend)
- SSH access (optional, for remote backends)

---

## 📦 Installation Methods

### Method 1: Binary Release (Recommended)

```bash
# Download latest release
curl -LO https://github.com/ecoPrimals/benchScale/releases/latest/download/benchscale-linux-amd64.tar.gz

# Extract
tar xzf benchscale-linux-amd64.tar.gz

# Install
sudo mv benchscale /usr/local/bin/
sudo chmod +x /usr/local/bin/benchscale

# Verify
benchscale --version
```

### Method 2: Build from Source

```bash
# Clone repository
git clone https://github.com/ecoPrimals/benchScale.git
cd benchScale

# Build release binary
cargo build --release

# Install
sudo cp target/release/benchscale /usr/local/bin/

# Verify
benchscale --version
```

### Method 3: Docker Container

```bash
# Build image
docker build -t benchscale:latest .

# Run
docker run --rm \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $(pwd)/topologies:/topologies \
  benchscale:latest create my-lab /topologies/simple-lan.yaml
```

### Method 4: Cargo Install

```bash
# Install from crates.io
cargo install benchscale

# Verify
benchscale --version
```

---

## ⚙️ Configuration

### Environment Variables

```bash
# Docker Backend
export BENCHSCALE_USE_HARDENED=true
export BENCHSCALE_DOCKER_TIMEOUT_SECS=60

# Libvirt Backend
export BENCHSCALE_LIBVIRT_URI="qemu:///system"
export BENCHSCALE_BASE_IMAGE_PATH="/var/lib/libvirt/images"
export BENCHSCALE_OVERLAY_DIR="/tmp/benchscale/overlays"

# SSH Configuration
export BENCHSCALE_SSH_USER="benchscale"
export BENCHSCALE_SSH_KEY="~/.ssh/id_rsa"
export BENCHSCALE_SSH_PORT=22
export BENCHSCALE_SSH_TIMEOUT_SECS=30

# Lab Configuration
export BENCHSCALE_STATE_DIR="/var/lib/benchscale"
export BENCHSCALE_DEFAULT_NETWORK_BRIDGE="br0"

# Logging
export RUST_LOG=info  # or debug, trace
```

### Configuration File

Create `~/.config/benchscale/benchscale.toml`:

```toml
[docker]
use_hardened_images = true
timeout_secs = 60

[libvirt]
uri = "qemu:///system"
base_image_path = "/var/lib/libvirt/images"
overlay_dir = "/tmp/benchscale/overlays"

[libvirt.ssh]
default_user = "benchscale"
key_path = "~/.ssh/id_rsa"
port = 22
timeout_secs = 30

[lab]
state_dir = "/var/lib/benchscale"
default_network_bridge = "br0"

[network]
timeout_secs = 60
```

---

## 🎯 Usage Examples

### Basic Lab Creation

```bash
# Create a simple 2-node lab
benchscale create my-lab topologies/simple-lan.yaml

# List active labs
benchscale list

# Get lab status
benchscale status my-lab

# Destroy lab
benchscale destroy my-lab
```

### Docker Backend (Default)

```bash
# Create Docker-based lab
benchscale create docker-lab topologies/simple-lan.yaml

# Execute command in node
benchscale exec docker-lab node-1 "ping -c 3 node-2"

# Get logs
benchscale logs docker-lab node-1

# Cleanup
benchscale destroy docker-lab
```

### Libvirt/KVM Backend

```bash
# Ensure libvirt is configured
export BENCHSCALE_LIBVIRT_URI="qemu:///system"
export BENCHSCALE_BASE_IMAGE_PATH="/var/lib/libvirt/images"

# Create VM-based lab
cargo run --features libvirt -- create vm-lab topologies/biomeos-p2p-test.yaml

# Monitor VMs
virsh list --all

# Cleanup
cargo run --features libvirt -- destroy vm-lab
```

### Remote/NUC Deployment

```bash
# Configure SSH access
export BENCHSCALE_SSH_USER="biomeos"
export BENCHSCALE_SSH_KEY="~/.ssh/nuc_key"

# Create topology with remote node
# See topologies/nuc-validation.yaml for example

# Deploy
benchscale create nuc-test topologies/nuc-validation.yaml

# Verify
benchscale status nuc-test
```

---

## 🐳 Docker Deployment

### Build Image

```bash
# Build
docker build -t benchscale:2.0.0 .

# Tag for registry
docker tag benchscale:2.0.0 ghcr.io/ecoprimals/benchscale:2.0.0
docker tag benchscale:2.0.0 ghcr.io/ecoprimals/benchscale:latest

# Push (requires authentication)
docker push ghcr.io/ecoprimals/benchscale:2.0.0
docker push ghcr.io/ecoprimals/benchscale:latest
```

### Run Container

```bash
# Run with Docker socket mounted
docker run --rm \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $(pwd)/topologies:/topologies \
  -v benchscale-state:/var/lib/benchscale \
  -e RUST_LOG=info \
  ghcr.io/ecoprimals/benchscale:latest \
  create my-lab /topologies/simple-lan.yaml

# Interactive shell
docker run -it --rm \
  -v /var/run/docker.sock:/var/run/docker.sock \
  ghcr.io/ecoprimals/benchscale:latest \
  /bin/sh
```

---

## 🔧 System Requirements

### Minimum

- **CPU:** 2 cores
- **RAM:** 2 GB
- **Disk:** 10 GB free
- **OS:** Linux (Ubuntu 20.04+, Debian 11+, Alpine 3.19+)

### Recommended

- **CPU:** 4+ cores
- **RAM:** 8+ GB
- **Disk:** 50 GB free SSD
- **OS:** Linux (Ubuntu 22.04 LTS recommended)

### For VM Backend

- **CPU:** Hardware virtualization (Intel VT-x/AMD-V)
- **RAM:** 16+ GB
- **Disk:** 100 GB free SSD
- **Additional:** libvirt, qemu-kvm installed

---

## 🔒 Security Considerations

### Permissions

```bash
# Create benchscale user
sudo useradd -r -s /bin/false benchscale

# Add to docker group (if using Docker backend)
sudo usermod -aG docker benchscale

# Add to libvirt group (if using libvirt backend)
sudo usermod -aG libvirt benchscale

# Set up state directory
sudo mkdir -p /var/lib/benchscale
sudo chown benchscale:benchscale /var/lib/benchscale
sudo chmod 755 /var/lib/benchscale
```

### SSH Keys

```bash
# Generate SSH key for VM access
ssh-keygen -t ed25519 -f ~/.ssh/benchscale_key -N ""

# Configure
export BENCHSCALE_SSH_KEY="~/.ssh/benchscale_key"

# Add to VMs
ssh-copy-id -i ~/.ssh/benchscale_key.pub user@vm-host
```

### Firewall

```bash
# Allow Docker (if using)
sudo ufw allow in on docker0

# Allow libvirt network (if using)
sudo ufw allow in on virbr0

# Allow SSH (if using remote backends)
sudo ufw allow 22/tcp
```

---

## 📊 Monitoring & Observability

### Logging

```bash
# Set log level
export RUST_LOG=debug

# Log to file
benchscale create my-lab topology.yaml 2>&1 | tee benchscale.log

# Structured logging (JSON)
export RUST_LOG=json
benchscale create my-lab topology.yaml > logs.json
```

### Health Checks

```bash
# Check benchscale binary
benchscale version

# Check Docker backend
docker ps

# Check libvirt backend
virsh list --all

# Check lab status
benchscale list
benchscale status my-lab
```

---

## 🚨 Troubleshooting

### Issue: "Cannot connect to Docker daemon"

```bash
# Check Docker is running
sudo systemctl status docker

# Check socket permissions
ls -la /var/run/docker.sock

# Add user to docker group
sudo usermod -aG docker $USER
newgrp docker
```

### Issue: "Failed to connect to libvirt"

```bash
# Check libvirt is running
sudo systemctl status libvirtd

# Check user permissions
sudo usermod -aG libvirt $USER
newgrp libvirt

# Verify connection
virsh -c qemu:///system list
```

### Issue: "SSH connection timeout"

```bash
# Test SSH manually
ssh -i ~/.ssh/benchscale_key user@host

# Check SSH config
export BENCHSCALE_SSH_TIMEOUT_SECS=60

# Verify SSH port
export BENCHSCALE_SSH_PORT=22
```

### Issue: "Lab creation failed"

```bash
# Check logs
export RUST_LOG=debug
benchscale create test topology.yaml

# Validate topology
cat topology.yaml

# Check disk space
df -h

# Check network
benchscale list
```

---

## 🔄 Upgrade Guide

### From 1.x to 2.0

1. **Backup existing labs**
   ```bash
   cp -r /var/lib/benchscale /var/lib/benchscale.backup
   ```

2. **Install new version**
   ```bash
   cargo install benchscale --force
   ```

3. **Update config**
   - Review new environment variables
   - Update topology files if needed

4. **Test**
   ```bash
   benchscale version
   benchscale create test-lab topologies/simple-lan.yaml
   benchscale destroy test-lab
   ```

---

## 📚 Additional Resources

- **Documentation:** https://github.com/ecoPrimals/benchScale/docs
- **Examples:** `/topologies` directory
- **Issues:** https://github.com/ecoPrimals/benchScale/issues
- **Discussions:** https://github.com/ecoPrimals/benchScale/discussions

---

## 🎯 Production Deployment Checklist

- [ ] **System Requirements Met**
  - [ ] Hardware specs adequate
  - [ ] OS version supported
  - [ ] Dependencies installed

- [ ] **Configuration Complete**
  - [ ] Environment variables set
  - [ ] Config file created (optional)
  - [ ] SSH keys configured (if needed)

- [ ] **Permissions Set**
  - [ ] benchscale user created
  - [ ] Docker/libvirt group membership
  - [ ] State directory permissions

- [ ] **Security Hardened**
  - [ ] Firewall rules configured
  - [ ] SSH keys protected (600 permissions)
  - [ ] Minimal container privileges

- [ ] **Testing Verified**
  - [ ] Version check passes
  - [ ] Backend connectivity confirmed
  - [ ] Sample lab creation successful
  - [ ] Cleanup works correctly

- [ ] **Monitoring Setup**
  - [ ] Logging configured
  - [ ] Health checks scheduled
  - [ ] Alerts configured (optional)

- [ ] **Backup Strategy**
  - [ ] State directory backed up
  - [ ] Config files version controlled
  - [ ] Recovery process documented

---

**Status:** Ready for Production Deployment ✅  
**Version:** 2.0.0  
**Last Updated:** December 27, 2025

---

*benchScale - Pure Rust Laboratory Substrate*

