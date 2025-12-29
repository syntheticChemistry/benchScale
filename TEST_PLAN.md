# benchScale IP Pool Integration - Test Plan
**Date:** December 29, 2025  
**Status:** Planning comprehensive test coverage

---

## Test Hierarchy

```
Unit Tests (Fast, No External Dependencies)
├── IP Pool Module                     ✅ 7 tests passing
├── CloudInit Network Config           🔲 TODO
└── Error Handling                     🔲 TODO

Integration Tests (Medium, Mock libvirt)
├── LibvirtBackend + IP Pool          🔲 TODO
├── CloudInit Generation              🔲 TODO
└── IP Lifecycle (Alloc/Release)      🔲 TODO

E2E Tests (Slow, Real libvirt/VMs)
├── Single VM Static IP               🔲 TODO
├── Multi VM Concurrent Creation      🔲 TODO
├── VM Deletion & IP Release          🔲 TODO
└── Pool Exhaustion Recovery          🔲 TODO
```

---

## Phase 1: Unit Tests (30 minutes)

### 1.1 CloudInit Network Config Tests
**File:** `src/cloud_init.rs` (add to existing tests module)

**Tests Needed:**
- [ ] `test_network_config_creation`
- [ ] `test_network_config_yaml_generation`
- [ ] `test_cloud_init_with_static_ip`
- [ ] `test_cloud_init_with_custom_dns`
- [ ] `test_network_config_serialization`

### 1.2 Error Handling Tests
**File:** `src/backend/ip_pool.rs` (add to existing tests module)

**Tests Needed:**
- [ ] `test_invalid_cidr_range`
- [ ] `test_range_outside_network`
- [ ] `test_release_unallocated_ip`
- [ ] `test_release_out_of_range_ip`

**Estimate:** 30 minutes to write and validate

---

## Phase 2: Integration Tests (1 hour)

### 2.1 LibvirtBackend + IP Pool Integration
**File:** `src/backend/libvirt_ip_pool_tests.rs` (new file)

**Tests Needed:**
- [ ] `test_backend_initialization_with_pool`
- [ ] `test_ip_allocation_on_vm_creation_mock`
- [ ] `test_ip_release_on_vm_deletion_mock`
- [ ] `test_concurrent_ip_allocation`
- [ ] `test_ip_persistence_across_operations`

**Approach:** Mock libvirt calls, test state management

### 2.2 CloudInit + IP Pool Integration
**File:** `src/cloud_init_integration_tests.rs` (new file)

**Tests Needed:**
- [ ] `test_cloud_init_receives_allocated_ip`
- [ ] `test_network_config_in_user_data`
- [ ] `test_network_config_yaml_format`
- [ ] `test_multiple_interfaces`

**Estimate:** 1 hour to write and validate

---

## Phase 3: E2E Tests (2 hours)

### 3.1 Real VM Tests
**File:** `tests/libvirt_e2e_tests.rs` (new file)

**Tests Needed:**
- [ ] `test_create_single_vm_with_static_ip`
- [ ] `test_create_two_vms_concurrent`
- [ ] `test_create_five_vms_concurrent`
- [ ] `test_vm_has_correct_static_ip`
- [ ] `test_vm_network_connectivity`
- [ ] `test_delete_vm_releases_ip`
- [ ] `test_recreate_vm_reuses_released_ip`

**Prerequisites:**
- Base VM image available
- Libvirt daemon running
- Sufficient storage space
- Network permissions

### 3.2 Stress Tests
**File:** `tests/libvirt_stress_tests.rs` (new file)

**Tests Needed:**
- [ ] `test_pool_exhaustion_handling`
- [ ] `test_rapid_create_delete_cycle`
- [ ] `test_10_concurrent_vms`
- [ ] `test_ip_leak_prevention`

**Estimate:** 2 hours (VMs take time to create/destroy)

---

## Test Execution Strategy

### Quick Validation (< 1 minute)
```bash
# Unit tests only (no external dependencies)
cargo test --lib ip_pool
cargo test --lib cloud_init
```

### Full Integration Tests (< 5 minutes)
```bash
# All tests except E2E
cargo test --features libvirt --lib
```

### Full E2E Suite (10-15 minutes)
```bash
# Everything including real VMs
cargo test --features libvirt -- --ignored --test-threads=1
```

---

## Test Data Requirements

### For Unit Tests
- ✅ No external data needed
- ✅ All data mocked inline

### For Integration Tests
- Test CIDR ranges (10.200.0.0/24)
- Mock SSH keys
- Test cloud-init configs

### For E2E Tests
- ✅ Base image: `/var/lib/libvirt/images/ubuntu-22.04-server-cloudimg-amd64.img`
- ✅ Available from agentReagents
- Test network: libvirt default (192.168.122.0/24)
- Disk space: 50GB minimum

---

## Success Criteria

### Unit Tests
- [x] All 7 IP pool tests passing ✅
- [ ] All 5 CloudInit network tests passing
- [ ] All 4 error handling tests passing
- **Target:** 16/16 unit tests passing

### Integration Tests
- [ ] All 5 backend integration tests passing
- [ ] All 4 CloudInit integration tests passing
- **Target:** 9/9 integration tests passing

### E2E Tests
- [ ] Single VM creation works
- [ ] 5 concurrent VMs work (no IP conflicts)
- [ ] IP release and reuse works
- [ ] No IP leaks detected
- **Target:** 7/7 E2E tests passing

### Overall Goal
**32 comprehensive tests covering all aspects of IP pool integration**

---

## Risk Mitigation

### Unit Tests
- **Risk:** None (fast, no dependencies)
- **Mitigation:** N/A

### Integration Tests
- **Risk:** Test isolation (state pollution)
- **Mitigation:** Use unique test prefixes, cleanup in `Drop`

### E2E Tests
- **Risk:** Flaky tests (timing, resource contention)
- **Mitigation:** 
  - Use `#[ignore]` by default
  - Add retry logic for VM operations
  - Generous timeouts
  - Proper cleanup in test teardown
  
- **Risk:** Disk space exhaustion
- **Mitigation:**
  - Clean up all test VMs
  - Use thin provisioning
  - Monitor disk usage

- **Risk:** Network conflicts with existing VMs
- **Mitigation:**
  - Use dedicated test IP range (192.168.122.200-220)
  - Check for existing VMs before tests
  - Unique VM name prefixes

---

## Implementation Order

1. **Phase 1a:** CloudInit network config unit tests (15 min)
2. **Phase 1b:** Error handling unit tests (15 min)
3. **Validate Phase 1:** Run all unit tests (1 min)
4. **Phase 2a:** Backend integration tests (30 min)
5. **Phase 2b:** CloudInit integration tests (30 min)
6. **Validate Phase 2:** Run integration tests (2 min)
7. **Phase 3a:** Single VM E2E test (30 min)
8. **Phase 3b:** Multi-VM E2E tests (1 hour)
9. **Phase 3c:** Stress tests (30 min)
10. **Validate Phase 3:** Full E2E suite (15 min)

**Total Estimated Time:** 3.5-4 hours

---

## Current Status

**Completed:**
- ✅ IP Pool unit tests (7/7)

**In Progress:**
- 🔲 CloudInit network config tests
- 🔲 Integration tests
- 🔲 E2E tests

**Next Action:**
Implement Phase 1a - CloudInit network config unit tests

---

## Test Coverage Goals

| Module | Current | Target |
|--------|---------|--------|
| IP Pool | 100% | 100% |
| CloudInit | ~40% | 95% |
| LibvirtBackend (IP) | 0% | 80% |
| E2E Flows | 0% | 70% |

**Overall Target:** 85%+ test coverage for IP pool integration

