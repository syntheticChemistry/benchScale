# LibvirtBackend → Backend Trait Migration Guide

**Date**: December 31, 2025  
**Goal**: Remove 397 hardcoded libvirt references  
**Pattern**: Dependency injection of `Backend` trait

## Migration Strategy

### Phase 1: Foundation ✅ (COMPLETE)
- Created `VmProvider` abstraction
- Integrated primal-substrate discovery
- Tests passing with mock backend

### Phase 2: Call Site Migration 🔄 (IN PROGRESS)
- Migrate ImageBuilder to accept Backend trait
- Update topology code to use discovery
- Refactor tests to use mock backends

### Phase 3: Complete Evolution
- Remove all direct LibvirtBackend::new() calls
- Use discovery throughout
- Support multiple backend types

---

## Migration Pattern

### Before (Hardcoded)

```rust
#[cfg(feature = "libvirt")]
use crate::backend::LibvirtBackend;

pub struct ImageBuilder {
    name: String,
    // ... fields
}

impl ImageBuilder {
    pub fn new(name: impl Into<String>) -> Result<Self> {
        // No backend - will create LibvirtBackend::new() later
        Ok(Self { name: name.into(), /* ... */ })
    }
    
    pub async fn build(self) -> Result<BuildResult> {
        // ❌ Hardcoded backend creation
        let backend = LibvirtBackend::new()?;
        backend.create_node(...).await?;
    }
}
```

### After (Trait-Based)

```rust
use crate::backend::Backend;
use std::sync::Arc;

pub struct ImageBuilder {
    name: String,
    backend: Arc<dyn Backend>,
    // ... fields
}

impl ImageBuilder {
    /// Create builder with any backend implementation
    pub fn new(name: impl Into<String>, backend: Arc<dyn Backend>) -> Result<Self> {
        Ok(Self { 
            name: name.into(),
            backend,
            // ...
        })
    }
    
    /// Create builder with discovered backend (zero hardcoding!)
    pub async fn with_discovery(
        name: impl Into<String>,
        discovery: &Discovery,
    ) -> Result<Self> {
        let provider = discovery
            .find_capability(Capability::VmProvisioning)
            .await?;
        
        // Connect to discovered provider
        let backend = connect_to_provider(&provider)?;
        
        Ok(Self {
            name: name.into(),
            backend,
            // ...
        })
    }
    
    pub async fn build(self) -> Result<BuildResult> {
        // ✅ Use injected backend
        self.backend.create_node(...).await?;
    }
}
```

---

## Migration Steps

### Step 1: Add Backend Field

```rust
// Before
pub struct MyStruct {
    name: String,
}

// After
pub struct MyStruct {
    name: String,
    backend: Arc<dyn Backend>,
}
```

### Step 2: Update Constructor

```rust
// Before
impl MyStruct {
    pub fn new(name: String) -> Result<Self> {
        Ok(Self { name })
    }
}

// After
impl MyStruct {
    // Accept any backend
    pub fn new(name: String, backend: Arc<dyn Backend>) -> Result<Self> {
        Ok(Self { name, backend })
    }
    
    // Optional: Discovery-based constructor
    pub async fn with_discovery(name: String, discovery: &Discovery) -> Result<Self> {
        let service = discovery.find_capability(Capability::VmProvisioning).await?;
        let backend = connect_to_provider(&service)?;
        Ok(Self { name, backend })
    }
}
```

### Step 3: Replace Direct Backend Calls

```rust
// Before
let backend = LibvirtBackend::new()?;
backend.create_node(...).await?;

// After
self.backend.create_node(...).await?;
```

### Step 4: Update Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    // Before
    #[tokio::test]
    async fn test_my_function() {
        let my_struct = MyStruct::new("test").unwrap();
        // Test requires libvirt running!
    }
    
    // After
    #[tokio::test]
    async fn test_my_function() {
        let mock_backend = Arc::new(MockBackend::new());
        let my_struct = MyStruct::new("test", mock_backend).unwrap();
        // Test works without external services!
    }
}
```

---

## File-by-File Migration Plan

### Priority 1: High-Impact Files

1. **src/image_builder.rs** (2 refs) ⬅️ **STARTING HERE**
   - Add `backend: Arc<dyn Backend>` field
   - Update constructors
   - Replace `LibvirtBackend::new()` calls
   - Update tests

2. **src/persistence/lifecycle.rs** (1 ref)
   - Accept backend in constructor
   - Use for VM operations

3. **src/backend/libvirt/utils.rs** (3 refs)
   - Make functions accept `&dyn Backend`
   - Remove direct backend creation

### Priority 2: Backend Implementation Files

4. **src/backend/libvirt/mod.rs** (11 refs)
   - Internal implementation (low priority)
   - These stay as LibvirtBackend methods

5. **src/backend/libvirt/backend_impl.rs** (2 refs)
   - Backend trait implementation
   - Keep as-is (implementation detail)

6. **src/backend/libvirt/vm_lifecycle.rs** (2 refs)
   - Internal libvirt methods
   - Keep as-is (implementation detail)

7. **src/backend/libvirt/vm_ready.rs** (4 refs)
   - Internal libvirt methods
   - Keep as-is (implementation detail)

### Priority 3: Tests

8. **src/backend/libvirt/libvirt_validation_tests.rs** (7 refs)
   - Integration tests
   - Can stay libvirt-specific (for libvirt validation)

9. **src/image_builder_improvements.rs** (1 ref)
   - Legacy code (consider removing)

---

## Example: ImageBuilder Migration

### Current Code Analysis

```rust
// Line 389-391: Cleanup after build
let backend = LibvirtBackend::new()?;
backend.delete_node(&vm_name).await?;

// Line 415-420: Create builder VM
let backend = LibvirtBackend::new()?;
backend.create_desktop_vm(name, base_image, &cloud_init, ...).await?;
```

### Migration Implementation

1. Add backend field to `ImageBuilder`:

```rust
pub struct ImageBuilder {
    name: String,
    base_image: Option<PathBuf>,
    memory_mb: u32,
    vcpus: u32,
    disk_size_gb: u32,
    steps: Vec<BuildStep>,
    cloud_init: Option<CloudInit>,
    backend: Arc<dyn Backend>,  // ← NEW
}
```

2. Update constructor:

```rust
impl ImageBuilder {
    /// Create with specific backend
    pub fn new(name: impl Into<String>, backend: Arc<dyn Backend>) -> Result<Self> {
        Ok(Self {
            name: name.into(),
            base_image: None,
            memory_mb: 4096,
            vcpus: 2,
            disk_size_gb: 35,
            steps: Vec::new(),
            cloud_init: None,
            backend,
        })
    }
    
    /// Create with discovered backend (zero hardcoding!)
    pub async fn with_discovery(name: impl Into<String>) -> Result<Self> {
        let discovery = Discovery::new().await?;
        let service = discovery.find_capability(Capability::VmProvisioning).await?;
        
        // For now, connect to libvirt if that's what we found
        // Later: Generic connection based on service metadata
        #[cfg(feature = "libvirt")]
        let backend = Arc::new(LibvirtBackend::new()?) as Arc<dyn Backend>;
        
        Self::new(name, backend)
    }
}
```

3. Use injected backend:

```rust
pub async fn build(self) -> Result<BuildResult> {
    // ...
    
    // Before: let backend = LibvirtBackend::new()?;
    // After: Use self.backend
    self.backend.delete_node(&vm_name).await?;
    
    // ...
}

async fn create_builder_vm(&self, name: &str, base_image: &Path) -> Result<NodeInfo> {
    // Before: let backend = LibvirtBackend::new()?;
    // After: Use self.backend
    self.backend.create_desktop_vm(...).await
}
```

---

## Testing Strategy

### Unit Tests (Mock Backend)

```rust
struct MockBackend;

#[async_trait]
impl Backend for MockBackend {
    async fn create_node(...) -> Result<NodeInfo> {
        Ok(NodeInfo { /* mock data */ })
    }
    // ... other methods
}

#[tokio::test]
async fn test_image_builder() {
    let backend = Arc::new(MockBackend);
    let builder = ImageBuilder::new("test", backend).unwrap();
    // Test without libvirt!
}
```

### Integration Tests (Real Backend)

```rust
#[tokio::test]
#[cfg(feature = "libvirt")]
async fn test_with_real_backend() {
    let backend = Arc::new(LibvirtBackend::new().unwrap());
    let builder = ImageBuilder::new("test", backend).unwrap();
    // Test with real libvirt
}
```

### Discovery Tests

```rust
#[tokio::test]
async fn test_with_discovery() {
    let builder = ImageBuilder::with_discovery("test").await.unwrap();
    // Automatically finds available backend
}
```

---

## Benefits

### Before Migration
- ❌ Hardcoded to libvirt
- ❌ Tests require libvirt running
- ❌ Can't support multiple backends
- ❌ Tight coupling

### After Migration
- ✅ Works with any Backend implementation
- ✅ Tests use mock backends
- ✅ Easy to add VMware, AWS, etc.
- ✅ Loose coupling via traits
- ✅ Discovery-based selection

---

## Progress Tracking

### Completed ✅
- [x] Create Backend trait abstraction
- [x] Create VmProvider wrapper
- [x] Integrate primal-substrate discovery
- [x] Write migration guide

### In Progress 🔄
- [ ] Migrate ImageBuilder (2 refs)
- [ ] Migrate persistence/lifecycle (1 ref)
- [ ] Migrate backend/libvirt/utils (3 refs)

### Pending
- [ ] Update all tests to use mock backends
- [ ] Create helper functions for backend connection
- [ ] Update documentation
- [ ] Performance optimization

---

## Common Pitfalls

### Pitfall 1: Forgetting to Update Tests

**Problem**: Tests still create `LibvirtBackend::new()` directly.

**Solution**: Use `MockBackend` or `#[cfg(feature = "libvirt")]` guards.

### Pitfall 2: Not Handling Backend Errors

**Problem**: Backend methods return `Result`, need proper error handling.

**Solution**: Use `?` operator and map errors appropriately.

### Pitfall 3: Arc<dyn Backend> Confusion

**Problem**: Trying to pass `&dyn Backend` instead of `Arc<dyn Backend>`.

**Solution**: Store as `Arc<dyn Backend>`, clone Arc when needed (cheap).

---

## Next Steps

1. **Complete ImageBuilder Migration** ⬅️ **NOW**
2. Migrate persistence/lifecycle
3. Migrate utility functions
4. Update all tests
5. Add discovery integration to CLI
6. Documentation updates

---

**This migration guide provides the pattern for all 397 references!**

