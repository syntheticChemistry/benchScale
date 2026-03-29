// SPDX-License-Identifier: AGPL-3.0-only
//! Cross-architecture binary resolution.
//!
//! Resolves primal binaries by target architecture from a
//! `plasmidBin`-style directory layout:
//!
//! ```text
//! primals/
//! ├── x86_64/
//! │   ├── beardog
//! │   ├── songbird
//! │   └── ...
//! └── aarch64/
//!     ├── beardog
//!     └── ...
//! ```

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Target architecture for binary resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Arch {
    /// x86_64 / amd64
    X86_64,
    /// aarch64 / arm64
    Aarch64,
}

impl Arch {
    /// Detect the current host architecture.
    pub fn host() -> Self {
        if cfg!(target_arch = "aarch64") {
            Self::Aarch64
        } else {
            Self::X86_64
        }
    }

    /// Directory name used in the plasmidBin layout.
    pub fn dir_name(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
        }
    }

    /// Parse from a string (accepts common aliases).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "x86_64" | "amd64" | "x64" => Some(Self::X86_64),
            "aarch64" | "arm64" => Some(Self::Aarch64),
            _ => None,
        }
    }
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.dir_name())
    }
}

/// Resolves primal binaries from a directory tree by architecture.
pub struct BinaryResolver {
    base_path: PathBuf,
    arch: Arch,
}

impl BinaryResolver {
    /// Create a resolver for the given base path and target architecture.
    pub fn new(base_path: impl Into<PathBuf>, arch: Arch) -> Self {
        Self {
            base_path: base_path.into(),
            arch,
        }
    }

    /// Create a resolver for the host architecture.
    pub fn for_host(base_path: impl Into<PathBuf>) -> Self {
        Self::new(base_path, Arch::host())
    }

    /// Resolve a primal binary by name.
    ///
    /// Checks `<base>/primals/<arch>/<name>` first, then
    /// `<base>/<arch>/<name>`, then `<base>/<name>`.
    pub fn resolve(&self, primal_name: &str) -> Result<PathBuf> {
        let candidates = [
            self.base_path
                .join("primals")
                .join(self.arch.dir_name())
                .join(primal_name),
            self.base_path
                .join(self.arch.dir_name())
                .join(primal_name),
            self.base_path.join(primal_name),
        ];

        for path in &candidates {
            if path.is_file() {
                return Ok(path.clone());
            }
        }

        Err(Error::Backend(format!(
            "binary '{primal_name}' not found for {} in {}",
            self.arch,
            self.base_path.display()
        )))
    }

    /// List all available primal binaries for this architecture.
    pub fn list_available(&self) -> Vec<String> {
        let arch_dir = self
            .base_path
            .join("primals")
            .join(self.arch.dir_name());

        if !arch_dir.is_dir() {
            let fallback = self.base_path.join(self.arch.dir_name());
            return Self::list_executables(&fallback);
        }

        Self::list_executables(&arch_dir)
    }

    fn list_executables(dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };

        entries
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_arch_host_detection() {
        let arch = Arch::host();
        if cfg!(target_arch = "aarch64") {
            assert_eq!(arch, Arch::Aarch64);
        } else {
            assert_eq!(arch, Arch::X86_64);
        }
    }

    #[test]
    fn test_arch_from_str_loose() {
        assert_eq!(Arch::from_str_loose("x86_64"), Some(Arch::X86_64));
        assert_eq!(Arch::from_str_loose("amd64"), Some(Arch::X86_64));
        assert_eq!(Arch::from_str_loose("aarch64"), Some(Arch::Aarch64));
        assert_eq!(Arch::from_str_loose("arm64"), Some(Arch::Aarch64));
        assert_eq!(Arch::from_str_loose("mips"), None);
    }

    #[test]
    fn test_arch_dir_name() {
        assert_eq!(Arch::X86_64.dir_name(), "x86_64");
        assert_eq!(Arch::Aarch64.dir_name(), "aarch64");
    }

    #[test]
    fn test_resolve_from_primals_subdir() {
        let tmp = TempDir::new().expect("tmpdir");
        let arch_dir = tmp.path().join("primals").join("x86_64");
        std::fs::create_dir_all(&arch_dir).expect("mkdir");
        std::fs::write(arch_dir.join("beardog"), b"ELF").expect("write");

        let resolver = BinaryResolver::new(tmp.path(), Arch::X86_64);
        let path = resolver.resolve("beardog").expect("resolve");
        assert!(path.ends_with("beardog"));
    }

    #[test]
    fn test_resolve_from_arch_subdir() {
        let tmp = TempDir::new().expect("tmpdir");
        let arch_dir = tmp.path().join("x86_64");
        std::fs::create_dir_all(&arch_dir).expect("mkdir");
        std::fs::write(arch_dir.join("songbird"), b"ELF").expect("write");

        let resolver = BinaryResolver::new(tmp.path(), Arch::X86_64);
        let path = resolver.resolve("songbird").expect("resolve");
        assert!(path.ends_with("songbird"));
    }

    #[test]
    fn test_resolve_not_found() {
        let tmp = TempDir::new().expect("tmpdir");
        let resolver = BinaryResolver::new(tmp.path(), Arch::X86_64);
        assert!(resolver.resolve("nonexistent").is_err());
    }

    #[test]
    fn test_list_available() {
        let tmp = TempDir::new().expect("tmpdir");
        let arch_dir = tmp.path().join("primals").join("x86_64");
        std::fs::create_dir_all(&arch_dir).expect("mkdir");
        std::fs::write(arch_dir.join("beardog"), b"ELF").expect("write");
        std::fs::write(arch_dir.join("songbird"), b"ELF").expect("write");

        let resolver = BinaryResolver::new(tmp.path(), Arch::X86_64);
        let available = resolver.list_available();
        assert_eq!(available.len(), 2);
    }
}
