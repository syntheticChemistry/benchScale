# Phantom Dependency Removal - primal-substrate

**Date**: January 9, 2026  
**Reason**: Never implemented, blocking builds, violates primal philosophy

---

## What Was `primal-substrate`?

A planned but never implemented discovery system for runtime capability discovery across benchScale, agentReagents, and other projects.

### Original Concept

```rust
// Planned API
pub trait Discovery {
    async fn find_capability(&self, cap: Capability) -> Result<ServiceInfo>;
    async fn discover_all(&self) -> Result<Vec<ServiceInfo>>;
}

pub trait Capability {
    fn name(&self) -> &str;
}
```

**Goal**: Zero-hardcoding discovery of VM providers, backends, and services.

---

## Why It Was Never Implemented

### 1. **Standard Solutions Already Exist**

- **mDNS/DNS-SD**: Local network service discovery (Apple Bonjour, Avahi)
- **Consul**: Distributed service registry and health checking
- **etcd**: Key-value store for service discovery
- **Kubernetes**: Service discovery built-in

### 2. **NIH Syndrome** (Not Invented Here)

Creating a custom discovery system would be:
- Reinventing solved problems
- Adding maintenance burden
- Reducing interoperability
- Violating primal philosophy: "use existing capabilities"

### 3. **Scope Creep**

benchScale is a **VM orchestration tool**, not a service discovery framework.

**Core mission**: Provision VMs for testing
**Out of scope**: Runtime service discovery infrastructure

---

## Why It Blocked Builds

### The Phantom Dependency

```toml
# benchScale/Cargo.toml (line 58)
primal-substrate = { path = "../primal-substrate" }

# agentReagents/Cargo.toml (line 42)
primal-substrate = { path = "../primal-substrate" }
```

**Problem**: Path doesn't exist!

**Impact**:
- `cargo build` fails
- Examples don't compile
- CI broken
- New users can't try the project

### How It Went Unnoticed

1. Working in incremental sessions (never clean builds)
2. Cargo caching dependency graph
3. Never cloned from fresh repo
4. **Lesson**: Need CI from clean state

---

## Discovery by biomeOS Team

**Credit**: biomeOS team discovered this while attempting to use benchScale for internet federation testing.

**Their handoff** (`SYNTHETICCHEMISTRY_TEAM_HANDOFF_JAN9.md`):
> "Both `benchScale` and `agentReagents` declare a dependency on `primal-substrate`, but this crate **doesn't exist**."

**Their recommendation**:
> "Option C - Use standard discovery, don't create custom substrate"

**✅ We agree 100%!**

---

## Resolution: Complete Removal

### Files Archived

**benchScale**:
- `src/backend/provider.rs` → `archive/phantom-dependencies/`
  - 338 lines of discovery wrapper code
  - Tests for discovery (mocked, never real)
  - VmProvider trait implementation

**agentReagents**:
- `src/discovery.rs` → `archive/phantom-dependencies/`
  - ReagentsProvider implementation
  - Similar discovery patterns

### Files Modified

**benchScale/Cargo.toml**:
```toml
# REMOVED:
# primal-substrate = { path = "../primal-substrate" }

# ADDED:
# Discovery: Use standard solutions (mDNS, DNS-SD, Consul)
# NOT creating custom substrate - primal philosophy is to use existing capabilities
```

**benchScale/src/backend/mod.rs**:
```rust
// REMOVED:
// pub mod provider;
// pub use provider::VmProvider;

// ADDED:
// Discovery: No custom provider module - use standard service discovery
// For runtime discovery, consumers should use:
// - mDNS/DNS-SD: Local network service discovery
// - Consul: Distributed service registry
// - Environment variables: Explicit configuration
```

**benchScale/src/image_builder.rs**:
- Removed `with_discovery()` method (stubbed, never worked)

**agentReagents/Cargo.toml**:
- Removed `primal-substrate` dependency

**agentReagents/src/lib.rs**:
- Removed `pub mod discovery;`

---

## Alternative Solutions

### For Consumers Who Need Discovery

#### Option 1: mDNS/DNS-SD (Recommended for Local Networks)

```rust
use dns_sd::{ServiceDaemon, RegisterService};

// Announce benchScale service
let service = ServiceDaemon::new()?.register(
    "_vm-provisioning._tcp",
    "benchScale",
    6000,  // port
)?;

// Discover services
let browse = ServiceDaemon::new()?.browse("_vm-provisioning._tcp")?;
for service in browse {
    println!("Found: {} at {}:{}", service.name, service.host, service.port);
}
```

**Crates**: `dns-sd`, `mdns`, `zeroconf`

#### Option 2: Consul (Recommended for Distributed)

```rust
use consul::Client;

// Register service
let client = Client::new("http://localhost:8500")?;
client.agent.register_service(
    "benchScale",
    Some("vm-provisioning"),
    6000,  // port
)?;

// Discover services
let services = client.catalog.services()?.await?;
for (name, tags) in services {
    if tags.contains(&"vm-provisioning".to_string()) {
        println!("Found: {}", name);
    }
}
```

**Crates**: `consul`, `consulate`

#### Option 3: Environment Variables (Simple & Explicit)

```bash
# Explicit configuration (no discovery needed)
export BENCHSCALE_BACKEND="libvirt"
export BENCHSCALE_LIBVIRT_URI="qemu:///system"
export BENCHSCALE_VM_IMAGES_DIR="/var/lib/libvirt/images"
```

**Recommendation**: Use this for single-host deployments.

---

## Primal Philosophy Alignment

### Before (Violated Principles)

- ❌ **Created custom substrate** instead of using existing
- ❌ **Added complexity** without clear benefit
- ❌ **Tight coupling** between projects via shared dependency
- ❌ **Blocked builds** with non-existent dependency

### After (Aligned with Principles)

- ✅ **Use existing capabilities** (mDNS, Consul, env vars)
- ✅ **Simple over complex** (env vars work great!)
- ✅ **Loose coupling** (projects are independent)
- ✅ **Builds work** (no phantom dependencies)

---

## Lessons Learned

### 1. **CI Must Build from Clean State**

```yaml
# .github/workflows/ci.yml
- name: Clean build test
  run: |
    cargo clean
    cargo build --all-targets
    cargo test
```

**Why**: Catches missing dependencies immediately.

### 2. **YAGNI (You Aren't Gonna Need It)**

Discovery sounded cool, but:
- No consumer actually needed it
- Standard solutions exist
- Added no real value
- Created maintenance burden

### 3. **Real Usage Finds Real Issues**

biomeOS team's attempt to use benchScale revealed this immediately.

**Lesson**: Get real consumers early!

### 4. **Document Non-Goals**

Should have documented from the start:
- "benchScale does NOT provide service discovery"
- "Use mDNS, Consul, or env vars for discovery"
- "Focus: VM orchestration, not discovery infrastructure"

---

## Future: If Discovery Is Actually Needed

### When to Consider Discovery

**Signs you need it**:
1. Deploying across multiple hosts
2. Services come and go dynamically
3. Manual configuration is error-prone
4. Need health checking and failover

**Don't build it if**:
1. Single host deployment (use env vars)
2. Static configuration works fine
3. Standard solutions exist (they do!)

### How to Do It Right

**Don't**:
- Create custom substrate
- Invent new protocols
- Add tight coupling

**Do**:
- Use standard mDNS/DNS-SD
- Integrate existing Consul
- Support env vars as fallback
- Document discovery patterns

---

## Summary

**What**: Removed phantom `primal-substrate` dependency  
**Why**: Never implemented, blocked builds, violated philosophy  
**How**: Archived discovery modules, updated dependencies  
**Alternative**: Use mDNS, Consul, or env vars  
**Lesson**: YAGNI + Use existing capabilities + CI from clean state  

**Status**: ✅ Complete - builds work again!

---

**Date**: January 9, 2026  
**Credit**: biomeOS team for discovery  
**Resolution**: Deep debt eliminated, primal philosophy restored

