# Primal Tools Architecture

**Date:** December 26, 2025  
**Document Type:** Architecture Definition

---

## 🎯 What Are Primal Tools?

**Primal Tools** are infrastructure components that serve the ecoPrimals ecosystem but are NOT primals themselves. They help build, test, deploy, and manage primals and biomes.

### Examples
- **benchScale** - Lab environment and testing system
- **bingoCube** - [Description TBD]
- **[Future tools]** - Deployment, monitoring, debugging

---

## 🆚 Primal vs Primal Tool

### Primals (Strict Sovereignty)

**Examples:** Songbird, BearDog, ToadStool, NestGate, Squirrel

**Requirements:**
- ✅ **Sovereignty First** - Own their interface and lifecycle
- ✅ **No Hardcoding** - Zero hardcoded endpoints, ports, dependencies
- ✅ **API-First** - Everything through documented APIs
- ✅ **Capability-Based** - Advertise capabilities, not implementation
- ✅ **Lifecycle Independence** - Can start/stop/evolve independently
- ✅ **Pure Delegation** - Never reimplement what others provide

**Philosophy:** "I am sovereign. I decide my interface."

---

### Primal Tools (Pragmatic Implementation)

**Examples:** benchScale, bingoCube

**Requirements:**
- ✅ **Pure Rust** - Preferred but not required
- ⚠️ **Code Sovereignty Violations OK** - Can depend on primals directly
- ⚠️ **Hardcoding Acceptable** - Test endpoints, default configs
- ⚠️ **Shell Scripts OK** - For VM management, deployment
- 🎯 **Ecosystem Service** - Serves developers and operators, not end-users

**Philosophy:** "I serve the ecosystem. I enable primals."

---

## 📋 Primal Tools Manifest

### 1. benchScale

**Purpose:** Lab environment and testing system  
**Type:** Infrastructure  
**Language:** Bash + YAML (VM management), Rust (future)  
**Repository:** git@github.com:ecoPrimals/benchScale.git  

**Sovereignty Violations:**
- Hardcoded primal names (Songbird, BearDog, etc.)
- Hardcoded default ports (3000, 9000, etc.)
- Direct primal binary dependencies
- Assumes specific primal capabilities

**Why It's OK:**
benchScale is a testing tool. It MUST know about primals to test them. Sovereignty violations are intentional and necessary.

**Key Features:**
- VM/container management
- Network simulation (latency, packet loss)
- Test scenario orchestration
- Multi-primal coordination testing

---

### 2. bingoCube

**Purpose:** [TBD]  
**Type:** [TBD]  
**Language:** [TBD]  
**Repository:** [TBD]

---

## 🏗️ Architecture Principles

### For Primal Tools

1. **Serve the Ecosystem**
   - Focus on developer/operator needs
   - Enable primal development and testing
   - Make deployment and management easier

2. **Pragmatic Over Dogmatic**
   - Use the right tool for the job
   - Shell scripts are fine for VM management
   - Hardcoding is OK for test scenarios
   - Direct dependencies are acceptable

3. **Pure Rust Preferred**
   - Core logic should be Rust when possible
   - Performance-critical paths in Rust
   - But don't force Rust where shell is better

4. **Documentation First**
   - Clear purpose and scope
   - Explicit about sovereignty violations
   - Easy to use and understand

---

## 🎯 Decision Framework

**Is this a Primal or Primal Tool?**

### It's a Primal if:
- ✅ End-users interact with it directly
- ✅ It provides domain-specific capabilities (compute, storage, AI, etc.)
- ✅ It needs to evolve independently
- ✅ Multiple instances may exist in an ecosystem
- ✅ Sovereignty is critical

**Examples:** Songbird (service mesh), BearDog (security), ToadStool (compute)

---

### It's a Primal Tool if:
- ✅ Developers/operators use it, not end-users
- ✅ It serves the ecosystem infrastructure
- ✅ It coordinates or tests multiple primals
- ✅ Sovereignty violations are pragmatic
- ✅ Single instance per deployment is typical

**Examples:** benchScale (testing), bingoCube (?), deployment tools

---

## 📊 Comparison Table

| Aspect | Primals | Primal Tools |
|--------|---------|--------------|
| **Users** | End-users, applications | Developers, operators |
| **Sovereignty** | Strict | Relaxed |
| **Hardcoding** | Never | Acceptable for tests |
| **Language** | Pure Rust | Rust + shell + whatever works |
| **Dependencies** | Minimal, abstract | Can depend on primals directly |
| **Lifecycle** | Independent | Coupled to ecosystem |
| **API Design** | Capability-based, abstract | Can be specific, concrete |
| **Evolution** | Must be backward compatible | Can break, it's tooling |

---

## 🚀 Directory Structure

### Current (Local Development)
```
ecoPrimals/phase2/
└── biomeOS/
    └── benchscale/        - Local development
```

### Future (Extracted)
```
ecoPrimals/
├── biomeOS/              - Core substrate
├── benchScale/           - Primal tool (extracted)
├── bingoCube/            - Primal tool
│
├── songbird/             - Primal
├── beardog/              - Primal
├── toadstool/            - Primal
├── nestgate/             - Primal
└── squirrel/             - Primal
```

---

## 📜 Guidelines for New Primal Tools

### 1. Clear Purpose
Document what problem it solves and why it exists.

### 2. Explicit Violations
List any sovereignty violations and why they're necessary.

### 3. Pure Rust Where Possible
Core logic should be Rust, but use the right tool for each job.

### 4. Ecosystem Service
Make it clear this serves developers/operators, not end-users.

### 5. Independent Repository
Each primal tool gets its own repo (once stable).

---

## 🎓 Examples

### benchScale Sovereignty Violations

**Acceptable:**
```rust
// benchScale can hardcode primal names for testing
const SONGBIRD_DEFAULT_PORT: u16 = 3000;
const BEARDOG_DEFAULT_PORT: u16 = 9000;

// Can assume specific primal capabilities
fn deploy_p2p_mesh() {
    let songbird = PrimalClient::new("songbird", 3000);
    let beardog = PrimalClient::new("beardog", 9000);
    // Direct coordination for testing
}
```

**NOT Acceptable in Primals:**
```rust
// ❌ Primals cannot hardcode other primals
const SONGBIRD_ENDPOINT: &str = "http://localhost:3000";  // NO!

// ✅ Primals use capability discovery
let discovery = discover_by_capability("service.discovery").await?;
```

---

## 🔮 Future Primal Tools

Potential future primal tools:
- **biomeDeployer** - Production deployment orchestration
- **primalMonitor** - Cross-primal monitoring and alerting
- **echoDebugger** - Distributed debugging across primals
- **lineageExplorer** - Visualize and manage lineage relationships

---

## 📝 Summary

**Primals:** Sovereign, independent, capability-based  
**Primal Tools:** Pragmatic, ecosystem-serving, can violate sovereignty

Both are essential. Primals provide capabilities. Primal Tools make those capabilities usable.

---

**Date:** December 26, 2025  
**Status:** ✅ Architecture Defined

