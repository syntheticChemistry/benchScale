# BiomeOS Integration with benchScale

**Date:** December 26, 2025  
**Status:** ✅ Complete (Local Development Phase)

---

## 🎯 Overview

BiomeOS can now orchestrate benchScale lab experiments programmatically! This validates benchScale as a primal tool and demonstrates the integration pattern.

---

## 🏗️ Architecture

### BiomeOS Lab Module

**Location:** `crates/biomeos-core/src/lab/mod.rs`

**Key Components:**
- `LabManager` - Orchestrates lab creation, deployment, testing
- `LabHandle` - Handle to a running lab environment
- `TestResult` - Results from lab tests

**Integration Pattern:**
```rust
use biomeos_core::lab::LabManager;

// Create lab manager
let lab_manager = LabManager::new();

// Create a lab
let lab = lab_manager.create_lab("simple-lan", "my-lab").await?;

// Deploy primals
lab.deploy("templates/p2p-secure-mesh.biome.yaml").await?;

// Run tests
let result = lab.run_test("btsp-tunnels").await?;

// Clean up
lab.destroy().await?;
```

---

## 🚀 Quick Start

### Run the Lab Experiment Demo

```bash
# From biomeOS root
cargo run --example lab_experiment
```

**What it does:**
1. Creates a simple-lan lab (2 nodes)
2. Deploys primals (Songbird, BearDog, ToadStool, NestGate)
3. Runs BTSP tunnel test
4. Verifies results
5. Cleans up

**Expected Output:**
```
╔════════════════════════════════════════════════════════════════╗
║  BiomeOS Lab Experiment Demo                                  ║
║  Testing benchScale integration                               ║
╚════════════════════════════════════════════════════════════════╝

📋 Experiment Plan:
   1. Create a simple-lan lab (2 nodes)
   2. Deploy primals (Songbird, BearDog, ToadStool, NestGate)
   3. Run BTSP tunnel test
   4. Verify results
   5. Clean up

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Step 1: Creating Lab Environment
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

✅ Lab created successfully!
   Name:     biomeos-experiment-01
   Topology: simple-lan

[... more output ...]

🎉 SUCCESS! BiomeOS successfully orchestrated a benchScale lab experiment!

✨ benchScale is working as a primal tool!
   Ready to push and separate when stable.
```

---

### Run the Full Lab Demo

```bash
# From biomeOS root
cargo run --example full_lab_demo
```

**What it does:**
- Runs 3 experiments in sequence
- Tests all major topologies
- Validates complete integration

---

## 📊 Integration Status

| Feature | Status | Notes |
|---------|--------|-------|
| Lab Creation | ✅ Complete | Via `LabManager::create_lab()` |
| Primal Deployment | ✅ Complete | Via `LabHandle::deploy()` |
| Test Execution | ✅ Complete | Via `LabHandle::run_test()` |
| Lab Cleanup | ✅ Complete | Via `LabHandle::destroy()` |
| Error Handling | ✅ Complete | Proper `Result<T>` types |
| Async Support | ✅ Complete | Full `async/await` |
| Logging | ✅ Complete | Via `tracing` |

---

## 🎓 Usage Examples

### Example 1: Simple Lab Test

```rust
use biomeos_core::lab::LabManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let manager = LabManager::new();
    
    // Create lab
    let lab = manager.create_lab("simple-lan", "test-lab").await?;
    
    // Run test
    let result = lab.run_test("btsp-tunnels").await?;
    
    if result.passed() {
        println!("✅ Test passed!");
    }
    
    // Cleanup
    lab.destroy().await?;
    
    Ok(())
}
```

### Example 2: Multiple Tests

```rust
use biomeos_core::lab::LabManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let manager = LabManager::new();
    let lab = manager.create_lab("p2p-3-tower", "multi-test").await?;
    
    // Run multiple tests
    let tests = vec!["p2p-coordination", "btsp-tunnels", "multi-tower-discovery"];
    
    for test in tests {
        let result = lab.run_test(test).await?;
        println!("{}: {}", test, if result.passed() { "PASS" } else { "FAIL" });
    }
    
    lab.destroy().await?;
    Ok(())
}
```

### Example 3: Custom benchScale Path

```rust
use biomeos_core::lab::LabManager;
use std::path::PathBuf;

let manager = LabManager::with_path(PathBuf::from("/custom/path/to/benchscale"));
let lab = manager.create_lab("simple-lan", "custom-lab").await?;
```

---

## 🔧 Prerequisites

### For Lab Experiments

1. **LXD Installed:**
   ```bash
   sudo snap install lxd
   sudo lxd init --minimal
   sudo usermod -aG lxd $USER
   newgrp lxd
   ```

2. **benchScale Available:**
   - Must be in `benchscale/` directory
   - Scripts must be executable: `chmod +x benchscale/scripts/*.sh`

3. **Primal Binaries (Optional):**
   - For full tests, place binaries in `../phase1bins/`
   - Tests will run without them (with warnings)

---

## 🧪 Testing

### Unit Tests

```bash
# Test lab module
cargo test --package biomeos-core lab::tests
```

### Integration Tests

```bash
# Run lab experiment demo
cargo run --example lab_experiment

# Run full lab demo
cargo run --example full_lab_demo
```

---

## 📝 Validation Criteria

**benchScale is ready to push and separate when:**

✅ **Criterion 1:** BiomeOS can create labs programmatically  
✅ **Criterion 2:** BiomeOS can deploy primals to labs  
✅ **Criterion 3:** BiomeOS can run tests and get results  
✅ **Criterion 4:** BiomeOS can clean up labs  
✅ **Criterion 5:** Integration is documented  
✅ **Criterion 6:** Examples work and demonstrate value  

**Status:** ✅ **ALL CRITERIA MET!**

---

## 🚀 Next Steps

### Now (Local Development)
- ✅ Integration complete
- ✅ Examples working
- ✅ Documentation complete

### When Ready to Push
```bash
cd benchscale/
git status
git add -A
git commit -m "Add BiomeOS integration examples"
git push -u origin main
```

### When Ready to Separate
1. Create `ecoPrimals/benchScale/` directory (parallel to biomeOS)
2. Move `benchscale/` contents there
3. Update biomeOS to reference external benchScale
4. Update documentation

---

## 💡 Design Notes

### Why This Integration Pattern?

**Primal Tool Philosophy:**
- benchScale serves BiomeOS (and other tools)
- BiomeOS orchestrates benchScale via shell scripts
- No tight coupling - just process execution
- Clean separation of concerns

**Benefits:**
- benchScale can evolve independently
- BiomeOS doesn't need to know benchScale internals
- Shell scripts provide stable interface
- Easy to test and debug

---

## 📚 Related Documentation

- [benchScale README](README.md) - Main documentation
- [benchScale QUICKSTART](QUICKSTART.md) - Getting started
- [PRIMAL_TOOLS_ARCHITECTURE](PRIMAL_TOOLS_ARCHITECTURE.md) - Architecture philosophy

---

**Integration Status:** ✅ Complete and validated!  
**Ready to Push:** ✅ Yes, when stable  
**Ready to Separate:** ⏳ After push is stable

---

*BiomeOS + benchScale = "Test like production, before production."* 🧪🚀

