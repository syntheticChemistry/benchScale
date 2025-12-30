# benchScale Refactoring Roadmap

**Created:** December 30, 2025  
**Status:** Architectural Planning Complete  
**Estimated Effort:** 25-30 hours

---

## Philosophy

> "Smart refactoring over mechanical splitting"  
> "Capability-based discovery over hardcoding"  
> "Fast AND safe Rust"

---

## Phase 1: Critical Fixes ✅ (COMPLETED)

### 1.1 Formatting & Linting ✅
- [x] Fix trailing whitespace
- [x] Run cargo fmt
- [x] Create constants module
- [x] Document architecture

### 1.2 Feature Gating ⏳
- [ ] Gate network module behind `experimental-network` feature
- [ ] Document experimental features
- [ ] Clean up 11 unimplemented!() stubs

---

## Phase 2: Smart Refactoring of libvirt.rs (4-6 hours)

### Current State
- **File:** `src/backend/libvirt.rs`
- **Lines:** 1557 (557 over limit)
- **Issue:** Monolithic file with mixed concerns

### Target Architecture

```
src/backend/libvirt/
├── mod.rs              (~300 lines) - Core Backend trait impl + orchestration
├── discovery.rs        (~200 lines) - Template & capability discovery
├── vm_lifecycle.rs     (~400 lines) - VM creation, start, stop, destroy
├── networking.rs       (~300 lines) - Network & IP management
├── cloud_init.rs       (~200 lines) - Cloud-init ISO generation
└── ssh.rs              (~150 lines) - SSH operations
```

### Responsibilities

#### mod.rs - Core Orchestration
- `LibvirtBackend` struct definition
- `Backend` trait implementation
- Module re-exports
- High-level coordination

#### discovery.rs - Capability Discovery
```rust
pub trait CapabilityDiscovery {
    async fn discover_storage_pools(&self) -> Result<Vec<StoragePool>>;
    async fn discover_networks(&self) -> Result<Vec<NetworkCapability>>;
    async fn discover_templates(&self) -> Result<HashMap<String, PathBuf>>;
}
```

#### vm_lifecycle.rs - VM Operations
- `create_vm()` - VM creation logic
- `start_vm()` - VM startup
- `stop_vm()` - VM shutdown
- `destroy_vm()` - VM deletion
- `get_vm_info()` - VM introspection

#### networking.rs - Network Management
- `create_network()` - Network creation
- `get_ip_address()` - IP discovery
- IP pool integration
- Network capability detection

#### cloud_init.rs - Provisioning
- `create_cloud_init_iso()` - ISO generation
- `write_user_data()` - User-data formatting
- `write_meta_data()` - Meta-data formatting
- Temporary file management

#### ssh.rs - Remote Operations
- `ssh_exec()` - Command execution
- `ssh_copy()` - File transfer
- Connection pooling
- Error handling

### Migration Strategy

1. **Create module structure**
   ```bash
   mkdir -p src/backend/libvirt
   touch src/backend/libvirt/{mod.rs,discovery.rs,vm_lifecycle.rs,networking.rs,cloud_init.rs,ssh.rs}
   ```

2. **Extract functions by responsibility**
   - Move template discovery → `discovery.rs`
   - Move VM lifecycle → `vm_lifecycle.rs`
   - Move networking → `networking.rs`
   - Move cloud-init → `cloud_init.rs`
   - Move SSH → `ssh.rs`

3. **Update imports**
   - Change `use crate::backend::libvirt::LibvirtBackend`
   - To `use crate::backend::libvirt::LibvirtBackend`
   - (No external API changes)

4. **Test after each module**
   ```bash
   cargo test --lib
   cargo test --test '*'
   ```

---

## Phase 3: Capability-Based Configuration (3-4 hours)

### Current Issues
- Hardcoded IPs: `192.168.122.10/24`
- Hardcoded paths: `/var/lib/libvirt/images`
- Hardcoded subnets: `10.100.0.0/24`

### Solution: Runtime Discovery

#### 3.1 Storage Discovery
```rust
// OLD: Hardcoded
let path = "/var/lib/libvirt/images";

// NEW: Capability-based
let storage_pool = backend
    .discover_storage_pools()?
    .find(|p| p.has_write_capability())
    .ok_or("No writable storage pool found")?;
let path = storage_pool.path();
```

#### 3.2 Network Discovery
```rust
// OLD: Hardcoded
let gateway = "192.168.122.1";

// NEW: Capability-based
let network = backend
    .discover_networks()?
    .find(|n| n.is_default())
    .ok_or("No default network found")?;
let gateway = network.gateway();
```

#### 3.3 XDG Base Directory Support
```rust
use crate::constants::paths;

// Respects XDG_DATA_HOME, XDG_RUNTIME_DIR
let images_dir = paths::libvirt_images_dir();
let temp_dir = paths::temp_dir();
```

### Implementation Steps

1. **Add discovery traits** (`discovery.rs`)
2. **Implement libvirt queries** (use `virt` crate APIs)
3. **Update all hardcoded paths** (use `constants::paths`)
4. **Add environment variable support**
5. **Test with different configurations**

---

## Phase 4: Error Handling Evolution (6-8 hours)

### Current Issues
- **145 `unwrap()` calls** - Can panic in production
- **37 `expect()` calls** - Better but still panics
- **1 `panic!()` call** - Needs review

### Solution: Idiomatic Error Handling

#### 4.1 Replace unwrap() with ?
```rust
// BAD
let value = operation().unwrap();

// GOOD
let value = operation()
    .context("Failed to perform operation")?;
```

#### 4.2 Use anyhow::Context
```rust
use anyhow::Context;

let config = load_config()
    .context("Failed to load configuration")?;
    
let vm = create_vm(&config)
    .context("Failed to create VM")?;
```

#### 4.3 Custom Error Types
```rust
#[derive(Debug, thiserror::Error)]
pub enum LibvirtError {
    #[error("VM not found: {0}")]
    VmNotFound(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
}
```

### Migration Strategy

1. **Audit all unwrap() calls**
   ```bash
   rg "\.unwrap\(\)" --type rust src/
   ```

2. **Categorize by context**
   - Test code: OK to keep
   - Infallible operations: Document why
   - Fallible operations: Replace with ?

3. **Add context to errors**
   ```rust
   .context(format!("Failed to create VM '{}'", name))?
   ```

4. **Test error paths**
   - Add tests for error conditions
   - Verify error messages are helpful

---

## Phase 5: Test Coverage to 90% (8-10 hours)

### Current Status
- **Tests:** 37 total (19 benchScale + 18 agentReagents)
- **E2E Tests:** 7 files
- **Coverage:** Unknown (need llvm-cov)

### Target: 90% Coverage

#### 5.1 Measure Current Coverage
```bash
cargo llvm-cov --all-features --workspace --html
open target/llvm-cov/html/index.html
```

#### 5.2 Add Unit Tests
Focus on:
- **discovery.rs** - Template and capability discovery
- **vm_lifecycle.rs** - VM operations
- **networking.rs** - Network operations
- **cloud_init.rs** - ISO generation
- **ssh.rs** - Remote operations

#### 5.3 Add Integration Tests
- Multi-VM creation
- Network isolation
- Cloud-init provisioning
- SSH operations

#### 5.4 Add Chaos Tests
- Network failures
- Disk full scenarios
- Timeout handling
- Concurrent operations

### Test Strategy

1. **Unit tests** - Each module >80% coverage
2. **Integration tests** - E2E scenarios
3. **Chaos tests** - Failure injection
4. **Performance tests** - Benchmarks

---

## Phase 6: Network Module Completion (Optional)

### Current State
- **11 `unimplemented!()` functions**
- **Status:** Experimental feature

### Options

#### Option A: Feature Flag (RECOMMENDED)
```toml
[features]
default = ["libvirt"]
libvirt = ["virt"]
experimental-network = []
```

```rust
#[cfg(feature = "experimental-network")]
pub mod network;
```

#### Option B: Complete Implementation
- Implement all 11 functions
- Add comprehensive tests
- Document network simulation capabilities
- **Effort:** 20+ hours

#### Option C: Remove Module
- Delete `src/network/mod.rs`
- Remove from `lib.rs`
- Document as future work

**Recommendation:** Option A - Feature flag until needed

---

## Phase 7: Verification in agentReagents (2-3 hours)

### Current Issue
```rust
// agentReagents/src/builder/mod.rs:374
todo!("Implement verification")
```

### Implementation

```rust
async fn verify_installation(vm: &VmHandle, manifest: &Manifest) -> Result<VerificationResult> {
    let mut results = Vec::new();
    
    // 1. Verify packages installed
    for package in &manifest.packages {
        let installed = vm.ssh_exec("ubuntu", &format!("dpkg -l | grep {}", package)).await?;
        results.push(VerificationCheck {
            name: format!("Package: {}", package),
            passed: !installed.is_empty(),
        });
    }
    
    // 2. Verify services running
    for service in &manifest.services {
        let status = vm.ssh_exec("ubuntu", &format!("systemctl is-active {}", service)).await?;
        results.push(VerificationCheck {
            name: format!("Service: {}", service),
            passed: status.trim() == "active",
        });
    }
    
    // 3. Verify desktop environment
    if manifest.desktop.is_some() {
        let display = vm.ssh_exec("ubuntu", "echo $DISPLAY").await?;
        results.push(VerificationCheck {
            name: "Desktop Environment".to_string(),
            passed: !display.is_empty(),
        });
    }
    
    let all_passed = results.iter().all(|r| r.passed);
    Ok(VerificationResult {
        passed: all_passed,
        checks: results,
    })
}
```

---

## Phase 8: Unsafe Code Review (1 hour)

### Current Status
- **1 unsafe block** found in codebase

### Action Items

1. **Locate unsafe block**
   ```bash
   rg "unsafe" --type rust src/
   ```

2. **Review necessity**
   - Is it truly required?
   - Can it be replaced with safe Rust?
   - Is it properly documented?

3. **Options**
   - **Eliminate:** Replace with safe alternative
   - **Justify:** Document why it's needed
   - **Isolate:** Move to dedicated module with safety docs

---

## Success Criteria

### Code Quality
- [ ] All files <1000 lines
- [ ] Zero unsafe blocks (or justified)
- [ ] <10 unwrap() in production code
- [ ] Zero unimplemented!() in production

### Testing
- [ ] 90% code coverage
- [ ] All E2E tests passing
- [ ] Chaos tests implemented
- [ ] Performance benchmarks

### Architecture
- [ ] Capability-based configuration
- [ ] Runtime discovery over hardcoding
- [ ] Modular, cohesive design
- [ ] Clear separation of concerns

### Documentation
- [ ] All public APIs documented
- [ ] Architecture diagrams
- [ ] Migration guides
- [ ] Examples updated

---

## Timeline

| Phase | Effort | Priority | Status |
|-------|--------|----------|--------|
| 1. Critical Fixes | 2h | HIGH | ✅ DONE |
| 2. Libvirt Refactor | 4-6h | HIGH | 📋 PLANNED |
| 3. Capability Config | 3-4h | HIGH | 📋 PLANNED |
| 4. Error Handling | 6-8h | MEDIUM | 📋 PLANNED |
| 5. Test Coverage | 8-10h | MEDIUM | 📋 PLANNED |
| 6. Network Module | 1h | LOW | 📋 PLANNED |
| 7. Verification | 2-3h | HIGH | 📋 PLANNED |
| 8. Unsafe Review | 1h | HIGH | 📋 PLANNED |
| **TOTAL** | **27-35h** | | |

---

## Next Steps

1. **Review this roadmap** with team
2. **Schedule refactoring sprint** (1 week)
3. **Execute phases 2-8** systematically
4. **Measure improvements** (coverage, performance)
5. **Document lessons learned**

---

**Status:** Ready for execution  
**Owner:** Development Team  
**Review Date:** January 6, 2026

