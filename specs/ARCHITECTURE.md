# benchScale - Architecture

> **Detailed technical architecture and design decisions**

## 🏛️ System Architecture

### High-Level Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     Application Layer                           │
│  (Lab Manager, Topology Builder, Custom Applications)           │
└────────────────────┬────────────────────────────────────────────┘
                     │
┌────────────────────▼────────────────────────────────────────────┐
│                    benchScale Library                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │  Backend     │  │  Cloud-Init  │  │ Persistence  │          │
│  │  Trait       │  │  Builder     │  │  Manager     │          │
│  └──────┬───────┘  └──────────────┘  └──────────────┘          │
│         │                                                        │
│  ┌──────▼───────┐  ┌──────────────┐  ┌──────────────┐          │
│  │  Libvirt     │  │  IP Pool     │  │   Lab        │          │
│  │  Backend     │  │  Manager     │  │  Registry    │          │
│  └──────┬───────┘  └──────────────┘  └──────────────┘          │
└─────────┼──────────────────────────────────────────────────────┘
          │
┌─────────▼──────────────────────────────────────────────────────┐
│                  System Layer                                   │
│   ┌──────────┐    ┌──────────┐    ┌──────────┐                │
│   │ libvirt  │    │   QEMU   │    │   KVM    │                │
│   └──────────┘    └──────────┘    └──────────┘                │
└─────────────────────────────────────────────────────────────────┘
```

## 📦 Module Architecture

### 1. Backend Layer

**Purpose:** Abstract hypervisor operations behind a common trait.

```rust
pub trait Backend {
    async fn create_vm(&self, config: VmConfig) -> Result<NodeInfo>;
    async fn start_vm(&self, name: &str) -> Result<()>;
    async fn stop_vm(&self, name: &str) -> Result<()>;
    async fn delete_vm(&self, name: &str) -> Result<()>;
    async fn get_vm_info(&self, name: &str) -> Result<NodeInfo>;
}
```

**Implementations:**
- `LibvirtBackend` - Production implementation using libvirt
- `MockBackend` - Test implementation (isolated to tests)

**Key Files:**
- `src/backend/mod.rs` - Trait definition
- `src/backend/libvirt.rs` - Libvirt implementation (1557 lines, needs refactoring)
- `src/backend/ip_pool.rs` - IP allocation management

### 2. Cloud-Init Layer

**Purpose:** Type-safe VM provisioning configuration.

```rust
CloudInit::builder()
    .add_user("ubuntu", "ssh-rsa AAAA...")
    .package("nginx")
    .package("postgresql")
    .static_ip("192.168.122.10", "192.168.122.1", "255.255.255.0")
    .run_command("systemctl start nginx")
    .build()
```

**Features:**
- Type-safe builder pattern
- No YAML parsing errors at runtime
- Compile-time verification
- Extensible for custom configurations

**Key Files:**
- `src/cloud_init/mod.rs` - Builder API
- `src/cloud_init/simplified.rs` - Simplified helpers

### 3. Persistence Layer

**Purpose:** Maintain VM state across system restarts.

**State Tracking:**
- VM lifecycle state (creating, running, stopped, etc.)
- IP allocations
- Resource mappings
- Metadata

**Storage:**
- JSON-based state files
- XDG Base Directory compliant
- Atomic writes for consistency
- Automatic recovery on startup

**Key Files:**
- `src/persistence/mod.rs` - Persistence manager
- `src/persistence/lifecycle.rs` - Lifecycle tracking

### 4. Lab Management Layer

**Purpose:** Multi-VM environment orchestration.

**Capabilities:**
- Create labs from topology definitions
- Start/stop all VMs in a lab
- Validate network connectivity
- Clean up resources on teardown

**Key Files:**
- `src/lab/mod.rs` - Lab creation and management
- `src/lab/registry.rs` - Lab registry and discovery

### 5. Network Topology Layer

**Purpose:** Define and validate network topologies.

**Features:**
- YAML-based topology definitions
- Subnet validation
- IP range validation
- VM role definitions
- Validation before deployment

**Key Files:**
- `src/topology/mod.rs` - Topology definition and validation

## 🔄 Data Flow

### VM Creation Flow

```
1. User calls create_desktop_vm()
   ↓
2. Backend allocates IP from IpPool
   ↓
3. CloudInit configuration generated
   ↓
4. Base image copied to new disk
   ↓
5. Disk resized to target size
   ↓
6. Cloud-init ISO created
   ↓
7. VM defined in libvirt
   ↓
8. VM started
   ↓
9. Wait for cloud-init completion
   ↓
10. SSH verification
    ↓
11. PersistenceManager updates state
    ↓
12. Return NodeInfo to user
```

### IP Allocation Flow

```
1. VM creation requested
   ↓
2. IpPool.allocate_ip(vm_name)
   ↓
3. Check if VM already has IP (idempotent)
   ↓
4. Find next available IP in subnet
   ↓
5. Mark IP as allocated
   ↓
6. Persist allocation to disk
   ↓
7. Return static IP
```

### State Persistence Flow

```
1. VM operation occurs
   ↓
2. PersistenceManager.update_state()
   ↓
3. Serialize state to JSON
   ↓
4. Atomic write to temp file
   ↓
5. Rename temp file to state file
   ↓
6. Ensure durability (fsync)
```

## 🧩 Component Interactions

### LibvirtBackend ↔ IpPool

```rust
// LibvirtBackend uses IpPool for deterministic IP allocation
impl LibvirtBackend {
    async fn create_desktop_vm(&self, name: &str, ...) -> Result<NodeInfo> {
        let ip = self.ip_pool.allocate_ip(name)?;
        // Use ip for cloud-init static network config
        ...
    }
}
```

### LibvirtBackend ↔ CloudInit

```rust
// CloudInit provides type-safe configuration
let cloud_init = CloudInit::builder()
    .add_user("ubuntu", ssh_key)
    .static_ip(&ip, gateway, netmask)
    .build();

// Backend generates ISO from CloudInit
backend.create_desktop_vm(name, image, &cloud_init, ...)?;
```

### LibvirtBackend ↔ PersistenceManager

```rust
// Backend notifies persistence of state changes
impl LibvirtBackend {
    async fn start_vm(&self, name: &str) -> Result<()> {
        // Start VM via libvirt
        domain.start()?;
        
        // Update persisted state
        self.persistence.update_vm_state(name, VmState::Running)?;
        
        Ok(())
    }
}
```

## 📊 State Management

### VM Lifecycle States

```
Idle
  ↓
Creating ──→ Error
  ↓
Starting
  ↓
WaitingForCloudInit
  ↓
Running ←→ Stopped
  ↓
Destroying
  ↓
Destroyed
```

### IP Pool States

```
Free → Allocated → Released → Free
              ↓
         (VM destroyed)
              ↓
         Cleanup → Free
```

## 🔐 Safety Guarantees

### Memory Safety
- **Zero unsafe code** - Enforced with `#![deny(unsafe_code)]`
- **Rust ownership model** - No use-after-free, no double-free
- **No data races** - Compile-time verification

### Type Safety
- **Builder patterns** - Compile-time validation
- **Strong typing** - No string-based configuration
- **Result types** - Explicit error handling

### Resource Safety
- **RAII** - Automatic resource cleanup
- **Drop trait** - Guaranteed cleanup
- **Async drop** - Proper async resource cleanup

### Thread Safety
- **Send + Sync** - Where appropriate
- **Arc for shared state** - Safe reference counting
- **Mutex for mutation** - Safe concurrent access

## 🚀 Performance Considerations

### Current Performance Characteristics

**VM Creation:**
- Base image copy: ~400ms (84MB → 30GB sparse)
- Cloud-init ISO generation: ~5ms
- VM definition: ~10ms
- Boot time: ~30s (waiting for cloud-init)
- SSH verification: ~5s

**IP Allocation:**
- Lookup: O(1) hash map access
- Allocation: O(n) scan of allocated IPs (n = VMs)
- Persistence: ~5ms write to disk

### Optimization Opportunities

1. **Reduce Clones** (77 instances)
   - Use references where possible
   - Use `Cow<str>` for string data
   - Share data with Arc when needed

2. **Reduce Allocations** (444 to_string() calls)
   - Pre-allocate strings
   - Use static strings where possible
   - Reduce temporary string creation

3. **Parallel Operations**
   - Parallel VM creation in labs
   - Concurrent health checks
   - Batched operations

4. **Zero-Copy**
   - Use `Bytes` for binary data
   - Memory-mapped files for large data
   - Streaming operations

## 🧪 Testing Strategy

### Unit Tests
- Test individual functions in isolation
- Mock external dependencies
- Fast (<1ms per test)
- **Current: 175 tests passing**

### Integration Tests
- Test component interactions
- Use real libvirt (dev environment)
- Slower (~100ms per test)
- **Current: 5 E2E tests**

### Chaos Tests
- Random VM operations
- Network failures
- Disk full scenarios
- **Planned: Not yet implemented**

### Property-Based Tests
- QuickCheck for invariants
- Fuzz testing for parsers
- State machine testing
- **Planned: Not yet implemented**

## 📈 Evolution Plan

### Phase 1: Refactoring (4-6 hours)
Split libvirt.rs into cohesive modules:
- `mod.rs` - Core orchestration (200 lines)
- `discovery.rs` - Template discovery (150 lines)
- `vm_lifecycle.rs` - VM operations (400 lines)
- `networking.rs` - Network management (300 lines)
- `cloud_init.rs` - Provisioning (250 lines)
- `ssh.rs` - Remote operations (200 lines)

### Phase 2: Error Handling (6-8 hours)
- Replace 145 unwrap() with ? operator
- Add anyhow::Context for better errors
- Implement custom error types where needed

### Phase 3: Test Coverage (8-10 hours)
- Measure with llvm-cov (target: 90%)
- Add missing unit tests
- Add chaos/fault injection tests

### Phase 4: Capability Discovery (3-4 hours)
- Storage pool discovery via libvirt API
- Network discovery
- Feature detection
- Runtime capability checks

### Phase 5: Performance (16-20 hours)
- Optimize clones and allocations
- Benchmarking suite
- Profiling and optimization
- Zero-copy where possible

---

**Total Evolution Time: 27-35 hours to 95%+ quality**

See [REFACTORING_ROADMAP.md](../REFACTORING_ROADMAP.md) for detailed plan.

