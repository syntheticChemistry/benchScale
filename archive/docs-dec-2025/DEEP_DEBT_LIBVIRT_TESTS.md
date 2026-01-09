# Deep Debt Analysis: Libvirt Test Socket Issues
**Date:** December 29, 2025  
**Status:** 🔍 **INVESTIGATION COMPLETE** - Solution Proposed

---

## 🐛 The Problem

### Symptoms
5 tests fail with:
```
Failed to connect to libvirt: error: Failed to connect socket to 
'/var/run/libvirt/libvirt-sock': Permission denied
```

### Failing Tests
```rust
// src/backend/libvirt_validation_tests.rs
test_backend_creation
test_wait_for_ssh_timeout_behavior
test_wait_for_cloud_init_timeout_behavior
test_exponential_backoff_ssh
test_wait_for_ip_private_helper
```

---

## 🔍 Root Cause Analysis

### What These Tests Are Actually Testing
```rust
#[tokio::test]
async fn test_wait_for_ssh_timeout_behavior() {
    let backend = LibvirtBackend::new().expect("Failed to create backend");  // ❌ Requires real libvirt!
    
    let result = backend.wait_for_ssh(
        "192.0.2.1",  // Unreachable TEST-NET IP
        "testuser",
        "testpass",
        Duration::from_secs(5),
    ).await;
    
    assert!(result.is_err(), "Expected timeout error");
    // Testing: timeout behavior, error messages
    // NOT testing: actual libvirt connection
}
```

### The Deep Debt

**Problem 1: Tight Coupling**
```rust
pub struct LibvirtBackend {
    conn: Arc<Mutex<Connect>>,  // ❌ Always requires real connection
    config: LibvirtConfig,
    ip_pool: IpPool,
    templates: HashMap<String, PathBuf>,
}

impl LibvirtBackend {
    pub fn new() -> Result<Self> {
        let conn = Connect::open(Some(&config.uri))?;  // ❌ Fails without libvirt socket!
        // ...
    }
}
```

**Problem 2: Private Helper Methods Can't Be Tested In Isolation**
```rust
// These are private methods on LibvirtBackend
async fn wait_for_ssh(...)
async fn wait_for_cloud_init(...)
async fn wait_for_ip(...)

// To test them, we must create a LibvirtBackend
// Which requires a real libvirt connection
// Which we don't need for testing timeout logic!
```

**Problem 3: No Abstraction for Testability**
- No trait for libvirt connection
- No dependency injection
- No mock capability
- Unit tests require full integration setup

---

## 🎯 Modern Rust Solution

### Principle: "Test Behavior, Not Implementation"

**What we want to test:**
- ✅ Timeout logic works correctly
- ✅ Error messages are meaningful
- ✅ Retry/backoff behavior
- ✅ Edge cases (unreachable hosts, invalid VMs)

**What we DON'T need:**
- ❌ Real libvirt daemon
- ❌ Actual network connections
- ❌ Real VMs

### Solution 1: Trait-Based Abstraction (Recommended)

**Step 1: Extract Connection Trait**
```rust
// src/backend/libvirt_conn.rs (NEW FILE)

/// Abstraction over libvirt connection for testability
#[async_trait]
pub trait LibvirtConnection: Send + Sync {
    async fn lookup_domain(&self, name_or_uuid: &str) -> Result<LibvirtDomain>;
    async fn create_domain(&self, xml: &str) -> Result<LibvirtDomain>;
    async fn list_domains(&self) -> Result<Vec<LibvirtDomain>>;
    async fn get_network_info(&self, name: &str) -> Result<NetworkInfo>;
}

/// Real implementation using virt crate
pub struct RealLibvirtConnection {
    conn: Arc<Mutex<Connect>>,
}

#[async_trait]
impl LibvirtConnection for RealLibvirtConnection {
    async fn lookup_domain(&self, name_or_uuid: &str) -> Result<LibvirtDomain> {
        let conn = self.conn.lock().await;
        let domain = Domain::lookup_by_name(&conn, name_or_uuid)?;
        // Convert to our abstraction
        Ok(LibvirtDomain { /* ... */ })
    }
    // ... other methods
}

/// Mock implementation for testing
#[cfg(test)]
pub struct MockLibvirtConnection {
    domains: Arc<Mutex<HashMap<String, MockDomain>>>,
}

#[cfg(test)]
#[async_trait]
impl LibvirtConnection for MockLibvirtConnection {
    async fn lookup_domain(&self, name_or_uuid: &str) -> Result<LibvirtDomain> {
        let domains = self.domains.lock().await;
        domains.get(name_or_uuid)
            .ok_or_else(|| Error::Backend("Domain not found".to_string()))
            .map(|d| d.to_libvirt_domain())
    }
    // ... other methods
}
```

**Step 2: Refactor LibvirtBackend**
```rust
pub struct LibvirtBackend {
    conn: Box<dyn LibvirtConnection>,  // ✅ Now mockable!
    config: LibvirtConfig,
    ip_pool: IpPool,
    templates: HashMap<String, PathBuf>,
}

impl LibvirtBackend {
    pub fn new() -> Result<Self> {
        Self::with_connection(Box::new(RealLibvirtConnection::new()?))
    }
    
    #[cfg(test)]
    pub fn with_mock_connection(conn: Box<dyn LibvirtConnection>) -> Self {
        Self {
            conn,
            config: LibvirtConfig::default(),
            ip_pool: IpPool::default_libvirt(),
            templates: HashMap::new(),
        }
    }
}
```

**Step 3: Testable Tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_wait_for_ssh_timeout_behavior() {
        let mock_conn = MockLibvirtConnection::new();
        let backend = LibvirtBackend::with_mock_connection(Box::new(mock_conn));
        
        // Now we can test without real libvirt!
        let result = backend.wait_for_ssh(
            "192.0.2.1",  // Still unreachable, but no libvirt needed
            "test", "test",
            Duration::from_secs(5),
        ).await;
        
        assert!(result.is_err());
    }
}
```

### Solution 2: Extract Testable Functions (Simpler, Faster)

**Step 1: Extract Timeout Logic to Pure Functions**
```rust
// src/backend/timeout_utils.rs (NEW FILE)

/// Exponential backoff configuration
#[derive(Debug, Clone)]
pub struct BackoffConfig {
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub max_attempts: usize,
    pub multiplier: f64,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            max_attempts: 20,
            multiplier: 1.5,
        }
    }
}

/// Retry a fallible async operation with exponential backoff
///
/// This is a pure, testable function that doesn't depend on libvirt.
pub async fn retry_with_backoff<F, Fut, T, E>(
    operation: F,
    config: BackoffConfig,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut delay = config.initial_delay;
    
    for attempt in 1..=config.max_attempts {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt == config.max_attempts => return Err(e),
            Err(_) => {
                tokio::time::sleep(delay).await;
                delay = std::cmp::min(
                    Duration::from_secs_f64(delay.as_secs_f64() * config.multiplier),
                    config.max_delay
                );
            }
        }
    }
    
    unreachable!()
}

/// Wait for a condition with timeout
///
/// Pure, testable function.
pub async fn wait_for_condition<F, Fut>(
    check: F,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<(), crate::Error>
where
    F: Fn() -> Fut,
    Fut: Future<Output = bool>,
{
    let start = Instant::now();
    
    while start.elapsed() < timeout {
        if check().await {
            return Ok(());
        }
        tokio::time::sleep(poll_interval).await;
    }
    
    Err(crate::Error::Backend(format!(
        "Condition not met after {:?}",
        timeout
    )))
}

// ============================================================================
// UNIT TESTS (No libvirt needed!)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_retry_with_backoff_success() {
        let mut attempts = 0;
        
        let result = retry_with_backoff(
            || async {
                attempts += 1;
                if attempts < 3 {
                    Err("not yet")
                } else {
                    Ok(42)
                }
            },
            BackoffConfig::default(),
        ).await;
        
        assert_eq!(result, Ok(42));
        assert_eq!(attempts, 3);
    }
    
    #[tokio::test]
    async fn test_retry_with_backoff_exhaustion() {
        let config = BackoffConfig {
            max_attempts: 3,
            ..Default::default()
        };
        
        let result = retry_with_backoff(
            || async { Err::<(), _>("always fails") },
            config,
        ).await;
        
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_wait_for_condition_success() {
        let mut counter = 0;
        
        let result = wait_for_condition(
            || async {
                counter += 1;
                counter >= 3
            },
            Duration::from_secs(10),
            Duration::from_millis(10),
        ).await;
        
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_wait_for_condition_timeout() {
        let result = wait_for_condition(
            || async { false },  // Never succeeds
            Duration::from_millis(100),
            Duration::from_millis(10),
        ).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Condition not met"));
    }
}
```

**Step 2: Use Pure Functions in LibvirtBackend**
```rust
impl LibvirtBackend {
    async fn wait_for_ssh(...) -> Result<()> {
        use crate::backend::timeout_utils::retry_with_backoff;
        
        retry_with_backoff(
            || async {
                // Try SSH connection
                SshClient::connect(ip, user, pass).await
            },
            BackoffConfig::default(),
        ).await
    }
}
```

### Solution 3: Mark as Integration Tests (Quickest Fix)

```rust
// Move to tests/libvirt_integration_tests.rs

#[tokio::test]
#[ignore]  // Requires actual libvirt daemon
async fn test_wait_for_ssh_timeout_behavior() {
    // Same test, but explicitly marked as integration test
}
```

---

## 📊 Comparison

| Solution | Effort | Benefits | Testability |
|----------|--------|----------|-------------|
| **1. Trait Abstraction** | High | Full mockability, best practices | ⭐⭐⭐⭐⭐ |
| **2. Pure Functions** | Medium | Simple, fast, testable | ⭐⭐⭐⭐ |
| **3. Mark #[ignore]** | Low | Quick fix, documents intent | ⭐⭐ |

---

## 🚀 Recommended Implementation Plan

### Phase 1: Immediate (Solution 3 - 10 minutes)
```rust
// src/backend/libvirt_validation_tests.rs
#[tokio::test]
#[ignore]  // Requires libvirt daemon with proper permissions
async fn test_wait_for_ssh_timeout_behavior() {
    // ... existing test
}
```

**Add at top of file:**
```rust
//! Integration tests for libvirt validation helpers
//!
//! These tests require:
//! - Libvirt daemon running
//! - User in libvirt group (sudo usermod -aG libvirt $USER)
//! - Or run with: sudo -E cargo test
//!
//! Run with: cargo test --features libvirt -- --ignored
```

### Phase 2: Extract Pure Functions (Solution 2 - 1-2 hours)
1. Create `src/backend/timeout_utils.rs`
2. Extract `retry_with_backoff` and `wait_for_condition`
3. Add comprehensive unit tests (no libvirt needed!)
4. Refactor LibvirtBackend to use these functions

### Phase 3: Trait Abstraction (Solution 1 - 3-4 hours)
1. Design LibvirtConnection trait
2. Create RealLibvirtConnection implementation
3. Create MockLibvirtConnection for tests
4. Refactor LibvirtBackend to use trait
5. Update all tests to use mocks

---

## 💡 Benefits of Modern Solution

### Before (Deep Debt)
```
❌ Tests coupled to libvirt socket
❌ Can't run in CI without complex setup
❌ Flaky tests (network/permission issues)
❌ Slow (real connection overhead)
❌ Hard to test edge cases
```

### After (Modern Rust)
```
✅ Tests run anywhere (no libvirt needed)
✅ Fast (no real connections)
✅ Deterministic (mocked responses)
✅ Easy to test edge cases
✅ CI-friendly
✅ Better separation of concerns
```

---

## 🎓 Rust Best Practices Applied

### 1. **Dependency Injection**
```rust
// Before
impl LibvirtBackend {
    pub fn new() -> Result<Self> {
        let conn = Connect::open(...)?;  // ❌ Hardcoded dependency
    }
}

// After
impl LibvirtBackend {
    pub fn new() -> Result<Self> {
        Self::with_connection(RealLibvirtConnection::new()?)
    }
    
    pub fn with_connection(conn: Box<dyn LibvirtConnection>) -> Self {
        // ✅ Dependency injected, testable
    }
}
```

### 2. **Trait Objects for Polymorphism**
```rust
pub trait LibvirtConnection: Send + Sync {
    // Common interface
}

// Production
let backend = LibvirtBackend::new();  // Uses RealLibvirtConnection

// Testing
let backend = LibvirtBackend::with_mock_connection(mock);  // Uses MockLibvirtConnection
```

### 3. **Pure Functions**
```rust
// Testable without any dependencies
pub async fn retry_with_backoff<F, Fut, T, E>(...) -> Result<T, E> {
    // Pure logic, no side effects
}

#[tokio::test]
async fn test_retry() {
    // Test the logic directly!
}
```

### 4. **Explicit Test Requirements**
```rust
#[tokio::test]
#[ignore]  // ✅ Documents what's needed
async fn test_real_libvirt() {
    // ...
}
```

---

## 📋 Action Items

### Immediate (Choose One)
- [ ] **Option A:** Mark tests as `#[ignore]` (10 min)
- [ ] **Option B:** Extract pure functions (1-2 hours)
- [ ] **Option C:** Full trait abstraction (3-4 hours)

### Recommended Path
1. ✅ **Now:** Mark as `#[ignore]` (unblock push)
2. 🔄 **Next Sprint:** Extract pure functions
3. 📅 **Future:** Consider trait abstraction if more mocking needed

---

## 🎯 Success Criteria

**After Implementation:**
- [ ] All tests run without libvirt daemon
- [ ] CI can run full test suite
- [ ] Test execution < 5 seconds
- [ ] No permission errors
- [ ] Code coverage > 85%
- [ ] Easy to add new tests

---

## 📚 References

**Rust Patterns:**
- [Dependency Injection in Rust](https://rust-unofficial.github.io/patterns/patterns/behavioural/strategy.html)
- [Testing with Traits](https://doc.rust-lang.org/book/ch10-02-traits.html)
- [async-trait crate](https://docs.rs/async-trait/)

**Similar Solutions:**
- `reqwest` uses trait for HTTP client (mockable)
- `sqlx` uses trait for database connections
- `tokio-postgres` trait-based connection pool

---

**Status:** Ready to implement  
**Recommendation:** Start with `#[ignore]`, evolve to pure functions  
**Effort:** 10 min (immediate) → 1-2 hours (complete)  
**Impact:** ✅ Unblocks push, improves testability, modern Rust patterns

