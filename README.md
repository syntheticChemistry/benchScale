# benchScale - BiomeOS Lab Environment System

**Version:** 1.0.0  
**Type:** Primal Tool (not a Primal)  
**Repository:** git@github.com:ecoPrimals/benchScale.git  
**Status:** 🚀 Active Development (Local Phase)

---

## 🎯 What is benchScale?

**benchScale** is a **primal tool** - infrastructure for testing and verifying BiomeOS deployments with real VMs and network simulation. Unlike primals (which maintain strict sovereignty), primal tools can have code sovereignty violations as they serve the ecosystem.

### Primal Tool vs Primal

**Primals** (e.g., Songbird, BearDog, ToadStool):
- ✅ Maintain strict sovereignty
- ✅ Own their own interface and lifecycle
- ✅ No hardcoded dependencies
- ✅ API-first design

**Primal Tools** (e.g., benchScale, bingoCube):
- ✅ Pure Rust (as much as possible)
- ⚠️ Can have code sovereignty violations
- ⚠️ Can depend on primals directly
- ⚠️ Can have hardcoded endpoints for testing
- 🎯 Serve the ecosystem, not end-users

---

## 🌱 Philosophy

**"Test like production, before production."**

benchScale provides:
- 🖥️ **VM Management** - Create, configure, destroy test labs
- 🌐 **Network Simulation** - Realistic latency, packet loss, NAT
- 🔐 **Security Testing** - Verify BTSP, BirdSong, lineage
- 📊 **Performance Testing** - Measure real-world performance
- ✅ **Deployment Verification** - Test before production

---

## 📦 Current Location

**Local Development**: `biomeOS/benchscale/`  
**Future Location**: `ecoPrimals/benchScale/` (parallel to biomeOS)

We're building benchScale locally within biomeOS for rapid iteration. Once stable, it will move to its own repository parallel to biomeOS.

```
ecoPrimals/
├── biomeOS/           - Core orchestration substrate
├── benchScale/        - Lab environment system (future)
├── bingoCube/         - [Another primal tool]
├── songbird/          - Primal: Service mesh
├── beardog/           - Primal: Security
├── toadstool/         - Primal: Compute
├── nestgate/          - Primal: Storage
└── squirrel/          - Primal: AI
```

---

## 🚀 Quick Start

```bash
# Install LXD (one-time setup)
sudo snap install lxd
sudo lxd init --minimal
sudo usermod -aG lxd $USER
newgrp lxd

# Create a lab
cd scripts/
./create-lab.sh --topology p2p-3-tower --name demo-lab

# Deploy primals
./deploy-to-lab.sh --lab demo-lab --manifest ../biome-templates/multi-tower-federation.biome.yaml

# Run tests
./run-tests.sh --lab demo-lab --test p2p-coordination

# Clean up
./destroy-lab.sh --lab demo-lab --force
```

---

## 🏗️ Architecture

### Network Topologies

1. **simple-lan** (2 nodes)
   - Purpose: Basic integration testing
   - Network: LAN (1ms latency, 1Gbps)

2. **p2p-3-tower** (3 nodes)
   - Purpose: Multi-tower P2P federation
   - Network: WAN (40-140ms latency, 50-100Mbps)
   - Geography: SF, NY, London

3. **nat-traversal** (4 nodes)
   - Purpose: NAT traversal and relay testing
   - Network: Mixed (public relay + NAT'd clients)

### Test Scenarios

- **p2p-coordination** - Test P2P mesh formation
- **btsp-tunnels** - Verify BTSP tunnel establishment
- **birdsong-encryption** - Test encrypted discovery
- **multi-tower-discovery** - Verify cross-tower communication
- **nat-traversal** - Test NAT hole punching
- **lineage-gated-relay** - Verify family-based access control
- **failure-recovery** - Test automatic failover

---

## 🧪 Network Simulation

Realistic network conditions:
- **Latency**: 1ms (LAN) to 140ms (WAN)
- **Jitter**: 5-10ms variance
- **Packet Loss**: 0% (LAN) to 1% (WAN)
- **Bandwidth**: 50Mbps to 1Gbps
- **NAT**: Multiple isolated subnets
- **Geography**: US West, US East, EU West

---

## 📊 Use Cases

### 1. Development Testing
Test new BiomeOS features before production deployment.

### 2. Integration Verification
Verify P2P coordination with real network conditions:
- BTSP tunnels across simulated WAN
- BirdSong encryption with realistic latency
- Multi-tower federation with geographic distribution

### 3. Performance Benchmarking
Measure real-world performance:
- Throughput under various conditions
- Latency with realistic delays
- Resource utilization

### 4. Security Auditing
Test security features:
- BTSP tunnel establishment
- BirdSong privacy-preserving discovery
- Lineage-gated relay access control

### 5. Training & Demos
Safe environment for learning and demonstrations.

---

## 🔧 Supported Hypervisors

- **LXD** (Primary) - Fast, native Linux containers
- **Docker** (Alternative) - Cross-platform containers
- **QEMU/KVM** (Future) - Full virtualization

---

## 📚 Documentation

- **[README.md](README.md)** - This file
- **[QUICKSTART.md](QUICKSTART.md)** - 5-minute getting started
- **[topologies/](topologies/)** - Network topology manifests
- **[scripts/](scripts/)** - VM management scripts

---

## 🎯 Roadmap

### Phase 1: Foundation (✅ COMPLETE)
- ✅ Architecture design
- ✅ Manifest format
- ✅ VM management scripts
- ✅ Network simulation design
- ✅ Documentation

### Phase 2: Core Features (NEXT)
- ⏳ Automated primal startup
- ⏳ Real test execution
- ⏳ Monitoring and metrics
- ⏳ Result reporting

### Phase 3: Advanced (FUTURE)
- ⏳ Chaos engineering
- ⏳ Performance profiling
- ⏳ Security auditing
- ⏳ CI/CD integration

### Phase 4: Extraction (FUTURE)
- ⏳ Move to parallel repository
- ⏳ Independent versioning
- ⏳ Standalone documentation

---

## 🤝 Contributing

benchScale is a primal tool, which means:
- ✅ Pure Rust preferred but not required
- ✅ Shell scripts are acceptable for VM management
- ✅ Can hardcode test endpoints
- ✅ Can depend on primals directly
- ✅ Focus on functionality over sovereignty

---

## 📜 License

Part of the ecoPrimals ecosystem.

---

## 🔗 Related Projects

- **[biomeOS](https://github.com/ecoPrimals/biomeOS)** - Core orchestration substrate
- **[Songbird](https://github.com/ecoPrimals/songbird)** - Service mesh primal
- **[BearDog](https://github.com/ecoPrimals/beardog)** - Security primal
- **[ToadStool](https://github.com/ecoPrimals/toadstool)** - Compute primal

---

## 📞 Support

For issues and questions:
- Create an issue on GitHub
- Check the [QUICKSTART.md](QUICKSTART.md) guide
- Review topology examples in [topologies/](topologies/)

---

**benchScale** - *Test like production, before production.* 🧪🚀

**Repository:** git@github.com:ecoPrimals/benchScale.git  
**Type:** Primal Tool  
**Status:** Local Development Phase
