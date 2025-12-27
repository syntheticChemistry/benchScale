# benchScale Enhancement Request - BiomeOS VM Support

**From:** BiomeOS Team  
**To:** benchScale Team  
**Date:** December 27, 2025  
**Priority:** High (Critical Path Item)  
**Timeline:** 5 days (December 27 - January 1, 2026)  
**Status:** 📋 Active Request

---

## 🎯 Executive Summary

**Goal:** Extend benchScale to support **BiomeOS VMs** as a backend (in addition to existing Docker backend).

**Why:** We need to validate complete BiomeOS deployments (boot + primals + networking) in VMs before deploying to physical NUCs. This enables reproducible testing of the full stack in a controlled environment.

**Impact:** Critical for BiomeOS NUC deployment validation. Blocks production deployment timeline.

---

## 📋 Requirements Overview

### Core Features Needed

1. **BiomeOS VM Backend** - Manage QEMU/KVM VMs via libvirt (you already started this!)
2. **Serial Console Capture** - Capture BootLogger output from VM serial console
3. **Root Filesystem Handling** - Support qcow2 disk images with copy-on-write overlays
4. **Network Bridge Management** - Real bridge networking with static IPs
5. **Health Monitoring** - Know when VMs are fully booted and ready

### Estimated Effort

- **New Code:** ~350 lines
- **Modified Code:** ~200 lines
- **Total Code Base:** ~2,500 lines (14% addition)
- **Complexity:** Medium (leverages existing libvirt code)

---

## 1. BiomeOS VM Backend (Priority 1)

### Current State

You already have `src/backend/libvirt.rs` (433 lines) with:
- ✅ Network management
- ✅ VM start/stop/delete
- ❌ VM creation (TODO at line 219)
- ❌ Log retrieval (TODO at line 390)

### What We Need

**VM Lifecycle Support:**

```yaml
# Example topology for BiomeOS VMs
topology:
  name: biomeos-federation
  backend: biomeos_vm  # New backend type
  
nodes:
  - name: tower-sf
    type: biomeos_vm
    image: /path/to/biomeos-root.qcow2  # Root filesystem image
    memory: 2G
    vcpus: 2
    network: biome-net
    serial_log: /tmp/tower-sf-serial.log  # Capture boot logs
```

**Key Requirements:**

1. **VM Creation** (complete TODO at line 219)
   - Load qcow2 disk image as root filesystem
   - Generate libvirt domain XML
   - Configure memory, vCPUs, network
   - Start VM
   - Wait for IP address (DHCP or static)

2. **Return VM Information**
   - VM name, UUID, state
   - IP address
   - Serial log path

**Implementation Notes:**

```rust
// Extend existing LibvirtBackend
impl Backend for LibvirtBackend {
    async fn create_node(
        &self,
        name: &str,
        image: &str,  // Path to qcow2 file
        network: &str,
        env: HashMap<String, String>,
    ) -> Result<NodeInfo> {
        // 1. Create copy-on-write overlay
        let overlay_path = create_disk_overlay(image, name)?;
        
        // 2. Generate libvirt XML
        let xml = generate_domain_xml(name, &overlay_path, memory, vcpus, network)?;
        
        // 3. Define and start VM
        let domain = Domain::define_xml(&self.conn, &xml)?;
        domain.create()?;
        
        // 4. Wait for IP (poll DHCP leases)
        let ip = wait_for_ip(name, timeout).await?;
        
        Ok(NodeInfo { /* ... */ })
    }
}
```

---

## 2. Serial Console Capture (Priority 1)

### Current State

Not implemented. No serial console support in libvirt XML generation.

### What We Need

**BiomeOS BootLogger Integration:**

BiomeOS uses BootLogger to write structured logs to the serial console during boot. We need to:

1. Configure QEMU to redirect serial output to a file
2. Read and parse serial logs for validation
3. Determine when boot is complete

**Example Serial Output:**

```
[2025-12-27 10:23:45] [Info] BiomeOS Init Starting
[2025-12-27 10:23:45] [Info] Filesystem: rootfs mounted (rw)
[2025-12-27 10:23:46] [Info] Network: eth0 configured (10.42.0.10/24)
[2025-12-27 10:23:47] [Info] Primal: Songbird started (PID 234)
[2025-12-27 10:23:48] [Info] Primal: BearDog started (PID 235)
[2025-12-27 10:23:49] [Success] BiomeOS Init Complete (178ms)
```

**Implementation:**

```rust
// In LibvirtBackend
pub struct BiomeOsVmNode {
    domain: Domain,
    serial_log_path: PathBuf,
}

impl BiomeOsVmNode {
    // Get serial console output
    pub fn get_serial_log(&self) -> Result<String> {
        std::fs::read_to_string(&self.serial_log_path)
    }
    
    // Parse boot completion
    pub fn is_boot_complete(&self) -> Result<bool> {
        let log = self.get_serial_log()?;
        Ok(log.contains("BiomeOS Init Complete"))
    }
    
    // Get boot time from log
    pub fn get_boot_time_ms(&self) -> Result<u64> {
        let log = self.get_serial_log()?;
        // Parse: "BiomeOS Init Complete (178ms)"
        parse_boot_time(&log)
    }
}
```

**libvirt XML Configuration:**

```xml
<domain type='kvm'>
  <name>tower-sf</name>
  <devices>
    <!-- Serial console to file -->
    <serial type='file'>
      <source path='/tmp/tower-sf-serial.log'/>
      <target type='isa-serial' port='0'/>
    </serial>
    <console type='file'>
      <source path='/tmp/tower-sf-console.log'/>
      <target type='serial' port='0'/>
    </console>
  </devices>
</domain>
```

---

## 3. Root Filesystem Handling (Priority 2)

### Current State

No disk image management. Need to support qcow2 files as root filesystems.

### What We Need

**Copy-on-Write Overlays:**

Like Docker image layers, we want:
- **Base image:** `/path/to/biomeos-base.qcow2` (read-only)
- **Per-VM overlay:** `/tmp/benchscale/tower-sf.qcow2` (read-write)

**Benefits:**
- Fast VM creation (no full disk copy)
- Isolated changes (each VM has own overlay)
- Easy cleanup (delete overlay file)

**Implementation:**

```rust
pub struct VmDiskConfig {
    pub source: PathBuf,           // Base qcow2 image
    pub size: String,              // "10G"
    pub format: DiskFormat,        // Qcow2, Raw, etc.
    pub copy_on_write: bool,       // Use backing files?
}

// Create copy-on-write overlay
fn create_disk_overlay(base_image: &Path, vm_name: &str) -> Result<PathBuf> {
    let overlay_path = format!("/tmp/benchscale/{}.qcow2", vm_name);
    
    // qemu-img create -f qcow2 -b base.qcow2 -F qcow2 overlay.qcow2
    Command::new("qemu-img")
        .args(&["create", "-f", "qcow2", "-b"])
        .arg(base_image)
        .args(&["-F", "qcow2"])
        .arg(&overlay_path)
        .output()?;
    
    Ok(PathBuf::from(overlay_path))
}

// Cleanup overlay on VM destroy
fn cleanup_disk_overlay(vm_name: &str) -> Result<()> {
    let overlay_path = format!("/tmp/benchscale/{}.qcow2", vm_name);
    std::fs::remove_file(overlay_path)?;
    Ok(())
}
```

**libvirt XML:**

```xml
<disk type='file' device='disk'>
  <driver name='qemu' type='qcow2'/>
  <source file='/tmp/benchscale/tower-sf.qcow2'/>
  <target dev='vda' bus='virtio'/>
</disk>
```

---

## 4. Network Bridge Management (Priority 2)

### Current State

You already have network creation in `src/backend/libvirt.rs:125-188`. Just needs bridge mode support.

### What We Need

**Bridge Networking with Static IPs:**

```yaml
network:
  type: bridge
  name: biome-br0
  subnet: 10.42.0.0/24
  dhcp: false  # Static IPs for primals
  
nodes:
  - name: tower-sf
    network:
      ip: 10.42.0.10/24
      gateway: 10.42.0.1
```

**Implementation:**

```rust
// Extend create_network to support bridge mode
async fn create_network(&self, name: &str, subnet: &str) -> Result<NetworkInfo> {
    let network_xml = format!(
        r#"<network>
  <name>{name}</name>
  <forward mode='nat'/>
  <bridge name='virbr-{bridge}' stp='on' delay='0'/>
  <ip address='{gateway}' netmask='255.255.255.0'>
    <!-- No DHCP - static IPs in topology -->
  </ip>
</network>"#,
        name = name,
        bridge = name.replace("-", ""),
        gateway = subnet.replace("/24", ".1"),
    );
    
    // Rest of implementation same...
}
```

**Key Requirements:**
- Create Linux bridge (already works)
- No DHCP (static IPs assigned in VM XML)
- VMs can reach each other
- VMs can reach host (for SSH)

---

## 5. Health Monitoring for VMs (Priority 3)

### Current State

Basic `is_available()` checks if libvirt is alive. Need VM-specific health checks.

### What We Need

**Multi-Level Health Checks:**

```rust
impl HealthCheck for BiomeOsVmNode {
    async fn is_healthy(&self) -> Result<HealthStatus> {
        let mut checks = vec![];
        
        // Check 1: VM is running (libvirt state)
        let vm_running = self.domain.is_active()?;
        checks.push(("vm_running", vm_running));
        
        // Check 2: Network is up (can ping)
        let network_up = self.ping().await.is_ok();
        checks.push(("network_up", network_up));
        
        // Check 3: BootLogger shows "Complete" (parse serial log)
        let boot_complete = self.is_boot_complete()?;
        checks.push(("boot_complete", boot_complete));
        
        // Check 4: Can SSH (optional)
        let ssh_ready = self.ssh_connect().await.is_ok();
        checks.push(("ssh_ready", ssh_ready));
        
        Ok(HealthStatus {
            healthy: checks.iter().all(|(_, ok)| *ok),
            checks,
        })
    }
}
```

**Usage:**

```rust
// Wait for VM to be fully ready
let lab = Lab::create("biomeos-test", topology, backend).await?;

for node in lab.nodes().await {
    loop {
        if node.is_healthy().await? {
            println!("{} is ready!", node.name);
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
```

---

## 🗓️ Suggested Timeline (5 Days)

### Day 1 (Dec 27): VM Creation Foundation

**Tasks:**
- [ ] Complete VM creation (TODO at line 219)
- [ ] Implement disk overlay creation
- [ ] Generate libvirt domain XML
- [ ] Test VM start/stop lifecycle

**Deliverable:** Can create and start VMs with qcow2 disk images

**Tests:**
```rust
#[test]
fn test_vm_creation() {
    let backend = LibvirtBackend::new()?;
    let node = backend.create_node("test-vm", "/path/to/base.qcow2", "test-net", HashMap::new()).await?;
    assert_eq!(node.status, NodeStatus::Running);
}
```

### Day 2 (Dec 28): VM Creation Completion

**Tasks:**
- [ ] Implement IP address waiting (poll DHCP leases)
- [ ] Add error handling for VM creation failures
- [ ] Add cleanup for failed VM creation
- [ ] Unit tests for disk overlay management

**Deliverable:** VM creation fully functional with error handling

**Sync Point:** BiomeOS team check-in - VM basics working?

### Day 3 (Dec 29): Serial Console Capture

**Tasks:**
- [ ] Add serial console to libvirt XML generation
- [ ] Implement `get_serial_log()` method
- [ ] Add log parsing utilities
- [ ] Test BootLogger output capture

**Deliverable:** Can capture and read serial console logs

**Tests:**
```rust
#[test]
fn test_serial_log_capture() {
    let node = start_test_vm().await?;
    tokio::time::sleep(Duration::from_secs(5)).await;
    let log = node.get_serial_log()?;
    assert!(log.contains("[Info] BiomeOS Init"));
}
```

**Sync Point:** BiomeOS team check-in - Can we see serial output?

### Day 4 (Dec 30): Networking & Health

**Tasks:**
- [ ] Implement bridge network support
- [ ] Add static IP assignment in VM XML
- [ ] Implement health check methods
- [ ] Test VM-to-VM connectivity

**Deliverable:** VMs can communicate, health checks work

**Tests:**
```rust
#[test]
fn test_vm_networking() {
    let lab = create_3_node_lab().await?;
    let result = lab.exec_on_node("node1", vec!["ping", "-c", "1", "node2"]).await?;
    assert_eq!(result.exit_code, 0);
}
```

### Day 5 (Dec 31): Integration & Documentation

**Tasks:**
- [ ] Full E2E integration test (3-node BiomeOS federation)
- [ ] Update README with BiomeOS VM examples
- [ ] Update SPECIFICATION.md with VM backend details
- [ ] Create example topology files

**Deliverable:** Complete BiomeOS VM support, documented

**Integration Test:**
```rust
#[tokio::test]
async fn test_biomeos_3_node_federation() {
    // Create 3 BiomeOS VMs
    let topology = Topology::from_file("topologies/biomeos-3-tower.yaml").await?;
    let backend = LibvirtBackend::new()?;
    let lab = Lab::create("biomeos-test", topology, backend).await?;
    
    // Wait for all VMs to boot
    for node in lab.nodes().await {
        wait_for_healthy(&node).await?;
    }
    
    // Verify BootLogger output
    for node in lab.nodes().await {
        let log = node.get_serial_log()?;
        assert!(log.contains("BiomeOS Init Complete"));
    }
    
    // Verify networking
    let result = lab.exec_on_node("tower-sf", vec!["ping", "-c", "1", "tower-ny"]).await?;
    assert_eq!(result.exit_code, 0);
    
    // Cleanup
    lab.destroy().await?;
}
```

**Sync Point:** BiomeOS team integration testing

---

## 🎯 Success Criteria

By end of Day 5, benchScale should support:

```bash
# Create BiomeOS VM topology
cat > biomeos-test.yaml << EOF
topology:
  name: biomeos-test
  backend: biomeos_vm
  
nodes:
  - name: tower-sf
    image: /path/to/biomeos.qcow2
    memory: 2G
    vcpus: 2
    network:
      ip: 10.42.0.10
    serial_log: /tmp/tower-sf-serial.log
EOF

# Deploy
benchscale create biomeos-test biomeos-test.yaml

# Status (should show boot complete)
benchscale status biomeos-test
# Output: tower-sf: RUNNING, IP: 10.42.0.10, Boot: Complete (178ms)

# Get BootLogger output
benchscale logs biomeos-test tower-sf
# Output: [Info] BiomeOS Init Complete (178ms)

# Cleanup
benchscale destroy biomeos-test
```

---

## 🤝 Coordination

### We Provide (BiomeOS Team)

- ✅ **BiomeOS root filesystem image** (qcow2 format)
  - Path: `/path/to/biomeos-root-v1.0.qcow2`
  - Size: ~2GB
  - Format: qcow2
  - Ready: Dec 27

- ✅ **BootLogger output format specification**
  - See: `biomeOS/BOOTLOGGER_PHASE1_SUCCESS.md`
  - Structured logs with timestamps
  - "BiomeOS Init Complete" signals boot done

- ✅ **Network configuration requirements**
  - Bridge mode preferred
  - Static IPs: 10.42.0.0/24
  - Gateway: 10.42.0.1

- ✅ **Health check criteria**
  - VM running
  - Network up (can ping)
  - BootLogger complete
  - SSH accessible

- ✅ **Integration test scenarios**
  - 3-node federation
  - P2P mesh connectivity
  - Primal startup sequence

### We Need (benchScale Team)

- 🔧 **VM backend working** (Day 2)
  - Can create VMs from qcow2
  - VMs start successfully
  - IP addresses returned

- 🔧 **Serial console capture** (Day 3)
  - Can read serial logs
  - Can parse BootLogger output

- 🔧 **Full integration ready** (Day 5)
  - 3-node topology works end-to-end
  - Health checks operational
  - Documentation complete

### Sync Schedule

- **Day 2 (Dec 28, 4pm):** VM basics check-in
- **Day 3 (Dec 29, 4pm):** Serial console check-in
- **Day 5 (Dec 31, 2pm):** Integration testing together

---

## 📚 Reference Materials

### Existing Code to Build On

- `benchscale/src/backend/libvirt.rs` - LibvirtBackend (433 lines, 60% done)
- `benchscale/src/backend/mod.rs` - Backend trait
- `benchscale/src/network/mod.rs` - Network simulation
- `benchscale/src/topology/mod.rs` - Topology management

### BiomeOS Resources

- `biomeOS/BOOTLOGGER_PHASE1_SUCCESS.md` - BootLogger format
- `biomeOS/specs/boot-observability.md` - Boot system specification
- `biomeOS/BENCHSCALE_TO_NUC_STRATEGY.md` - Overall strategy

### libvirt Examples

**Serial Console Configuration:**
```xml
<domain type='kvm'>
  <devices>
    <serial type='file'>
      <source path='/tmp/vm-serial.log'/>
      <target type='isa-serial' port='0'/>
    </serial>
  </devices>
</domain>
```

**Disk with Backing File:**
```xml
<disk type='file' device='disk'>
  <driver name='qemu' type='qcow2'/>
  <source file='/tmp/overlay.qcow2'/>
  <backingStore type='file'>
    <format type='qcow2'/>
    <source file='/path/to/base.qcow2'/>
  </backingStore>
  <target dev='vda' bus='virtio'/>
</disk>
```

---

## 💡 Why This Matters

### Current Pain Points

- **BiomeOS:** Can boot VMs manually, but no automation
- **benchScale:** Can deploy to Docker, but can't test full OS
- **Testing:** No reproducible way to validate BiomeOS before NUC deployment

### With BiomeOS VM Support

- ✅ **Automated BiomeOS VM deployment**
- ✅ **Full boot-to-federation testing**
- ✅ **Reproducible validation before NUC deployment**
- ✅ **Same tool for containers AND VMs**
- ✅ **Production-quality testing infrastructure**

### Impact

**De-risks BiomeOS NUC deployment** by validating everything in VMs first.  
**Makes benchScale the universal testing tool** for the entire ecoPrimals ecosystem.

---

## 🚀 Getting Started

### Accept This Request?

**If Yes:**
1. Reply with confirmed timeline (5 days realistic?)
2. Assign owner (who's driving this?)
3. Confirm sync schedule (Day 2, 3, 5 check-ins)
4. We'll provide BiomeOS disk images immediately

### Questions?

Reach out anytime! We're coordinating closely and can provide:
- BiomeOS disk images for testing
- BootLogger output examples
- Integration test scenarios
- Debugging help
- Pair programming sessions

---

## 📞 Contact

**From:** BiomeOS Team  
**Repository:** `../../biomeOS/`  
**Slack/Discord:** #benchscale-vm-support  
**Emergency Contact:** See BiomeOS README

---

**Summary:** Extend benchScale LibvirtBackend to support BiomeOS VMs. ~350 new lines, 5-day timeline. Enables full BiomeOS validation before NUC deployment.

**This is the critical path for BiomeOS production deployment.** 🚀

---

**Request Status:** 📋 Active  
**Priority:** High  
**Timeline:** 5 days  
**Dependencies:** None (libvirt code already started)  
**Blocking:** BiomeOS NUC deployment

