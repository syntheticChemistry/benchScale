# benchScale Specifications

**Version:** 2.0.0  
**Status:** Active Development  
**Last Updated:** December 27, 2025

---

## 📚 Specification Documents

This directory contains the authoritative specifications for benchScale development.

### Core Specifications

1. **[SPECIFICATION.md](./SPECIFICATION.md)** - Complete technical specification
   - Purpose & scope
   - Architecture
   - API contracts
   - Data models
   - Integration patterns

2. **[DEVELOPMENT_STATUS.md](./DEVELOPMENT_STATUS.md)** - Current development state
   - What's implemented
   - What's in progress
   - What's planned
   - Known issues
   - Timeline

3. **[BIOMEOS_VM_SUPPORT_REQUEST.md](./BIOMEOS_VM_SUPPORT_REQUEST.md)** - Active enhancement request
   - From: BiomeOS Team
   - Priority: High
   - Timeline: 5 days
   - Needed for BiomeOS NUC deployment validation

---

## 🎯 Quick Reference

### What is benchScale?

benchScale is a **pure Rust laboratory substrate** for distributed system testing. It provides:

- Declarative YAML topologies
- Multiple backend support (Docker, libvirt/KVM)
- Network simulation (latency, packet loss, bandwidth)
- Test scenario orchestration
- Integration with BiomeOS and other ecoPrimals

### Current Status

**Version:** 2.0.0  
**Maturity:** Early Development (C+ grade)  
**Production Ready:** No (8-12 weeks away)  
**Test Coverage:** ~5% (target: 90%)

### Active Work

**Current Priority:** BiomeOS VM support (5-day sprint)
- Extend libvirt backend
- Add serial console capture
- Disk image management
- VM health monitoring

See [BIOMEOS_VM_SUPPORT_REQUEST.md](./BIOMEOS_VM_SUPPORT_REQUEST.md) for details.

---

## 📋 Document Status

| Document | Status | Last Updated | Owner |
|----------|--------|--------------|-------|
| SPECIFICATION.md | ✅ Complete | Dec 27, 2025 | benchScale Team |
| DEVELOPMENT_STATUS.md | ✅ Complete | Dec 27, 2025 | benchScale Team |
| BIOMEOS_VM_SUPPORT_REQUEST.md | 📋 Active Request | Dec 27, 2025 | BiomeOS Team |

---

## 🔄 Change Process

### Proposing Changes

1. Create issue or discussion
2. Draft spec change
3. Review with stakeholders
4. Update specification
5. Implement changes
6. Update DEVELOPMENT_STATUS.md

### Versioning

Specifications follow semantic versioning:
- **Major** (2.0.0): Breaking changes to API or architecture
- **Minor** (2.1.0): New features, backward compatible
- **Patch** (2.0.1): Bug fixes, clarifications

---

## 🤝 Related Documentation

### In This Repository

- **Root Documentation:**
  - `../README.md` - User-facing documentation
  - `../QUICKSTART.md` - Getting started guide
  - `../PRIMAL_TOOLS_ARCHITECTURE.md` - Primal Tool philosophy
  - `../BIOMEOS_INTEGRATION.md` - Integration with BiomeOS

- **Development Documentation:**
  - `../COMPREHENSIVE_AUDIT_REPORT.md` - Code audit (Dec 27, 2025)
  - `../ACTION_PLAN.md` - 12-week development roadmap
  - `../EXECUTIVE_SUMMARY.md` - Quick overview

### External References

- **BiomeOS:** `../../biomeOS/` - Parent project
- **Phase 2 Architecture:** `../../ARCHITECTURE.md` - Overall Phase 2 design
- **Primal Tools:** See PRIMAL_TOOLS_ARCHITECTURE.md for context

---

## 📞 Contact

**Team:** benchScale Development Team  
**Repository:** `ecoPrimals/benchScale` (future)  
**Current Location:** `phase2/benchscale/`  
**Status:** Pre-extraction (local to BiomeOS for now)

---

**Next Steps:** See [BIOMEOS_VM_SUPPORT_REQUEST.md](./BIOMEOS_VM_SUPPORT_REQUEST.md) for immediate priorities.

