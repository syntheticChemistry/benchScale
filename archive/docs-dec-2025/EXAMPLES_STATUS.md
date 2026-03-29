# Examples Status - benchScale

**Date**: January 9, 2026  
**Status**: Partial cleanup completed  
**Priority**: LOW (examples are for demonstration, not critical)  

---

## Current Status

### ✅ Working Examples

These examples should compile and run:
- `vm_cleanup.rs` - Uses cleanup module (working)

### 🔄 Needs Minor Updates

These examples need backend parameter added:
- `build_working_desktop.rs` - **UPDATED** ✅
- `build_cosmic_image.rs` - Needs `Arc<LibvirtBackend>` parameter
- `build_from_existing_improved.rs` - Needs `Arc<LibvirtBackend>` parameter  
- `production_vm_ready.rs` - Needs `Arc<LibvirtBackend>` parameter
- `cloud_init_integration_test.rs` - Needs `Arc<LibvirtBackend>` parameter

### 📦 Archived (Obsolete APIs)

These examples use APIs that no longer exist:
- `lab_status.rs` - Uses `backend::lab` module (commented out)
- `create_from_cosmic_template.rs` - Uses old `VmConfig` API

**Location**: `archive/examples-needs-lab-module/`

---

## How to Fix Remaining Examples

### Pattern to Apply

**Before** (Old API):
```rust
use benchscale::{ImageBuilder};

let builder = ImageBuilder::new("my-vm")?;
```

**After** (Current API):
```rust
use benchscale::{ImageBuilder, LibvirtBackend};
use std::sync::Arc;

let backend = Arc::new(LibvirtBackend::new()?);
let builder = ImageBuilder::new("my-vm", backend)?;
```

### Example Fix Commands

```bash
# For each remaining example, add these lines after imports:
# use benchscale::LibvirtBackend;
# use std::sync::Arc;

# Then before ImageBuilder::new(), add:
# let backend = Arc::new(LibvirtBackend::new()?);

# And update the call:
# ImageBuilder::new("name", backend)?
```

---

## Why Examples Broke

### Root Cause: API Evolution

**Evolution #16** (December 2025): ImageBuilder became backend-agnostic
- Old: `ImageBuilder::new(name)` - Assumed libvirt
- New: `ImageBuilder::new(name, backend)` - Explicit backend

**Why**: Support multiple backends (libvirt, Docker, cloud providers)

**Impact**: All examples using `ImageBuilder` need updates

### Root Cause: Discovery Module Removal

**Evolution #24** (January 2026): Removed phantom primal-substrate
- Removed: `with_discovery()` method
- Removed: Custom discovery system
- Why: Never implemented, NIH syndrome

**Impact**: Examples using discovery need updates

---

## Recommended Actions

### Option 1: Fix Remaining Examples (2-3 hours)

**Steps**:
1. Update each example with backend parameter
2. Test each example manually
3. Add to CI: `cargo check --examples --features libvirt`

**Benefits**:
- Complete example coverage
- Users can learn from working code
- CI catches API breakage

**Cost**: 2-3 hours of work

### Option 2: Archive Remaining Examples (5 minutes)

**Steps**:
1. Move remaining broken examples to archive
2. Create note explaining why
3. Point users to working examples in agentReagents

**Benefits**:
- Quick solution
- Focuses effort on critical issues
- agentReagents has better examples anyway

**Cost**: Users lose benchScale examples

### Option 3: Minimal Fix (30 minutes)

**Steps**:
1. Fix ONE example (build_working_desktop.rs) - **DONE** ✅
2. Add comment to others: "See build_working_desktop.rs for current API"
3. Archive obsolete examples

**Benefits**:
- Users have one working example
- Shows current API pattern
- Low effort

**Cost**: Partial coverage

---

## Recommendation: Option 3 (COMPLETE ✅)

**Reasoning**:
1. One working example is sufficient for API demonstration
2. agentReagents has comprehensive examples
3. Time better spent on critical issues (permission model, tests)
4. Can revisit later if users request more examples

**Status**: ✅ `build_working_desktop.rs` updated and working

---

## CI Integration

### Add to `.github/workflows/ci.yml`

```yaml
# Check examples compile
- name: Check examples
  run: |
    cargo check --examples --features libvirt
    cargo check --example build_working_desktop --features libvirt
```

**Benefits**:
- Catches API breakage early
- Ensures examples stay current
- Low cost (fast check)

---

## Future Improvements

### When Time Permits

1. **Update All Examples** (2-3 hours)
   - Fix remaining 4 examples
   - Test manually
   - Add to CI

2. **Add New Examples** (1-2 hours each)
   - User session libvirt example
   - Custom directory example
   - Multi-VM orchestration
   - Network testing

3. **Example Tests** (3-4 hours)
   - Convert examples to integration tests
   - Mock backends for CI
   - Automated testing

4. **Example Documentation** (1-2 hours)
   - README for examples/
   - Quick start guide
   - API migration guide

---

## For Users

### Where to Find Working Examples

**agentReagents** has comprehensive, working examples:
- `agentReagents/templates/*.yaml` - Template manifests
- `agentReagents/src/bin/agent-reagents.rs` - CLI usage
- `agentReagents/README.md` - Quick start guide

**benchScale** working example:
- `benchScale/examples/build_working_desktop.rs` - ImageBuilder API ✅

### Quick Start

```bash
# Option 1: Use agentReagents (recommended)
cd agentReagents
cargo run --bin agent-reagents build templates/biomeos-tower-simple.yaml

# Option 2: Use benchScale example
cd benchScale
cargo run --example build_working_desktop --features libvirt
```

---

## Status Summary

- **Working**: 1 example (build_working_desktop.rs) ✅
- **Needs Updates**: 4 examples (low priority)
- **Archived**: 2 examples (obsolete APIs)
- **CI Check**: Recommended but not critical
- **User Impact**: Minimal (agentReagents has better examples)

**Recommendation**: Accept current state, revisit if users request more examples.

---

**Priority**: LOW  
**Effort**: 2-3 hours to fix all  
**Value**: Medium (nice-to-have)  
**Decision**: DEFERRED to future sprint  

✅ One working example sufficient for now!

