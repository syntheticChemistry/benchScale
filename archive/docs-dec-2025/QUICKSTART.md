# benchScale Quickstart Guide

Get started with benchScale in 5 minutes!

---

## 🚀 Quick Example

```bash
cd benchscale/scripts/

# 1. Create a 3-tower P2P lab
./create-lab.sh --topology p2p-3-tower --name demo-lab

# 2. Deploy primals
./deploy-to-lab.sh --lab demo-lab --manifest ../../templates/multi-tower-federation.biome.yaml

# 3. Run tests
./run-tests.sh --lab demo-lab --test p2p-coordination

# 4. Clean up
./destroy-lab.sh --lab demo-lab --force
```

---

## 📋 Prerequisites

### Option 1: LXD (Recommended)
```bash
# Install LXD
sudo snap install lxd

# Initialize LXD
sudo lxd init --minimal

# Add your user to lxd group
sudo usermod -aG lxd $USER
newgrp lxd
```

### Option 2: Docker
```bash
# Install Docker
curl -fsSL https://get.docker.com | sh

# Add your user to docker group
sudo usermod -aG docker $USER
newgrp docker
```

### Primal Binaries
```bash
# Make sure you have primal binaries in ../phase1bins/
ls ../phase1bins/
# Should show: songbird-bin, beardog-bin, toadstool-bin, nestgate-bin, squirrel-bin
```

---

## 🎯 Common Workflows

### Simple LAN Test
```bash
cd benchscale/scripts/

# Create 2-node LAN lab
./create-lab.sh --topology simple-lan --name lan-test

# Deploy primals
./deploy-to-lab.sh --lab lan-test --manifest ../../templates/p2p-secure-mesh.biome.yaml

# Test BTSP tunnels
./run-tests.sh --lab lan-test --test btsp-tunnels

# Clean up
./destroy-lab.sh --lab lan-test
```

### NAT Traversal Test
```bash
cd benchscale/scripts/

# Create 4-node NAT lab
./create-lab.sh --topology nat-traversal --name nat-test

# Deploy primals
./deploy-to-lab.sh --lab nat-test --manifest ../../templates/lineage-gated-relay.biome.yaml

# Test NAT traversal
./run-tests.sh --lab nat-test --test nat-traversal

# Test lineage-gated relay
./run-tests.sh --lab nat-test --test lineage-gated-relay

# Clean up
./destroy-lab.sh --lab nat-test
```

### Multi-Tower Federation
```bash
cd benchscale/scripts/

# Create 3-tower lab
./create-lab.sh --topology p2p-3-tower --name federation-test

# Deploy primals
./deploy-to-lab.sh --lab federation-test --manifest ../../templates/multi-tower-federation.biome.yaml

# Test multi-tower discovery
./run-tests.sh --lab federation-test --test multi-tower-discovery

# Test P2P coordination
./run-tests.sh --lab federation-test --test p2p-coordination

# Test all
./run-tests.sh --lab federation-test --test all

# Clean up
./destroy-lab.sh --lab federation-test
```

---

## 🔍 Troubleshooting

### LXD Not Found
```bash
# Install LXD
sudo snap install lxd
sudo lxd init --minimal
```

### Permission Denied
```bash
# Add your user to lxd group
sudo usermod -aG lxd $USER
newgrp lxd
```

### Cannot Copy Binaries
```bash
# Check if primal binaries exist
ls -la ../phase1bins/

# If not, pull them
cd ../phase1bins
./pull-primals.sh
```

### VMs Not Starting
```bash
# Check LXD status
lxc list

# Restart LXD
sudo systemctl restart snap.lxd.daemon

# Check logs
journalctl -u snap.lxd.daemon -n 50
```

---

## 📊 Monitoring

### List Running Labs
```bash
# List all LXD containers
lxc list

# List labs (by naming pattern)
lxc list | grep "demo-lab"
```

### Check VM Status
```bash
# Check specific VM
lxc info demo-lab-sf-tower

# Execute command in VM
lxc exec demo-lab-sf-tower -- ps aux
```

### View Logs
```bash
# View VM logs
lxc exec demo-lab-sf-tower -- tail -f /var/log/syslog

# View primal logs (if running)
lxc exec demo-lab-sf-tower -- tail -f /root/songbird.log
```

---

## 🎓 Next Steps

1. **Read the full README**: [README.md](README.md)
2. **Explore topologies**: [topologies/](topologies/)
3. **Review test scenarios**: Check available tests with `./run-tests.sh --help`
4. **Create custom topologies**: Copy and modify existing `.yaml` files

---

## 💡 Tips

- **Fast iteration**: Use `simple-lan` topology for quick tests
- **Realistic testing**: Use `p2p-3-tower` for production-like scenarios
- **Security testing**: Use `nat-traversal` for lineage and relay tests
- **Clean up**: Always destroy labs when done to save resources

---

**benchScale** - *Test like production, before production.* 🧪🚀

