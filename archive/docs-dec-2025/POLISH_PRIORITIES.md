# Polish & Testing Priorities

**Date:** December 29, 2025

---

## Immediate Issues Found

### 1. Failing Unit Test ❌
**Test:** `config::tests::test_env_var_ssh_port`  
**Issue:** Environment variable not being read correctly  
**Priority:** HIGH (blocks clean test run)

### 2. E2E Tests Don't Compile ❌
**File:** `tests/libvirt_e2e_tests.rs`  
**Issues:**
- Type inference errors with `tokio::join!`
- Import issues
**Priority:** HIGH (these tests validate IP pool integration)

### 3. Test Isolation Issues
**Issue:** Tests may interfere with each other via environment variables  
**Priority:** MEDIUM

---

## Action Plan

### Phase 1: Fix Failing Tests (30 min)
1. ✅ Identify failing test
2. 📋 Fix environment variable handling
3. 📋 Ensure test isolation
4. 📋 Get to 117/117 passing

### Phase 2: Fix E2E Test Compilation (30 min)
1. 📋 Simplify E2E tests (remove complex type inference)
2. 📋 Fix imports
3. 📋 Ensure they compile (even if `#[ignore]`)
4. 📋 Document how to run them

### Phase 3: Add Integration Tests (45 min)
1. 📋 Test IP allocation in `create_desktop_vm()`
2. 📋 Test IP release in `delete_node()`
3. 📋 Test concurrent VM creation
4. 📋 Test pool exhaustion

### Phase 4: Documentation & Polish (30 min)
1. 📋 Document IP pool configuration
2. 📋 Add troubleshooting guide
3. 📋 Improve error messages
4. 📋 Final validation

---

## Time Budget: 2-2.5 hours
## Goal: Production-ready, fully tested IP pool integration

Let's proceed systematically!

