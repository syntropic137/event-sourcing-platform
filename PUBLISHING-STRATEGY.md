# Publishing Strategy - Multi-Language Monorepo

**Date:** November 5, 2025  
**Status:** Planning

---

## Overview

This monorepo contains **10 publishable packages** across 4 languages/platforms:
- **3 TypeScript/Node.js packages** (npm/GitHub Packages)
- **5 Rust crates** (crates.io)
- **2 Python packages** (PyPI)
- **1 VSCode extension** (VS Marketplace)

---

## 📦 Package Inventory

### TypeScript/Node.js Packages

#### 1. `@event-sourcing-platform/typescript`
**Location:** `event-sourcing/typescript/`  
**Current Version:** 0.1.0  
**Status:** ✅ READY TO PUBLISH  
**Registry:** GitHub Package Manager (configured)  
**Description:** Event Sourcing SDK with @CommandHandler pattern

**Publishing:**
- ✅ CI/CD configured
- ✅ Repository URL updated
- ✅ publishConfig set
- ✅ Tests passing
- ✅ Tag v0.1.0 created

---

#### 2. `@eventstore/sdk-ts`
**Location:** `event-store/sdks/sdk-ts/`  
**Current Version:** 0.2.0  
**Status:** ⚠️ PRIVATE (currently)  
**Registry:** GitHub Package Manager (when published)  
**Description:** TypeScript SDK for gRPC Event Store

**Decision Needed:**
- Currently marked `private: true`
- Used as workspace dependency only
- **Recommendation:** Keep private OR publish separately

---

#### 3. `vsa-vscode`
**Location:** `vsa/vscode-extension/`  
**Current Version:** 0.1.0  
**Status:** 🔄 READY FOR VS MARKETPLACE  
**Registry:** Visual Studio Marketplace  
**Description:** VSCode extension for VSA validation

**Publishing:**
- Already packaged: `vsa-vscode-0.1.0.vsix`
- Needs VS Marketplace publisher account
- Use `vsce publish` command

---

### Rust Crates

#### 4. `event-sourcing-rust`
**Location:** `event-sourcing/rust/`  
**Current Version:** 0.1.0  
**Status:** 🔄 ALPHA - READY FOR CRATES.IO  
**Registry:** crates.io  
**Description:** Rust SDK for Event Sourcing patterns

**Publishing:**
- ⚠️ Repository URL is placeholder: `https://github.com/yourorg/event-sourcing-platform`
- Needs: Update Cargo.toml with correct repo URL
- Command: `cargo publish --dry-run` (test), `cargo publish`

---

#### 5. `vsa-cli`
**Location:** `vsa/vsa-cli/`  
**Current Version:** 0.1.0 (workspace)  
**Status:** 🔄 READY FOR CRATES.IO  
**Registry:** crates.io  
**Description:** CLI tool for VSA management

**Publishing:**
- Part of Cargo workspace
- Binary: `vsa`
- Command: `cargo publish -p vsa-cli`

---

#### 6. `vsa-core`
**Location:** `vsa/vsa-core/`  
**Current Version:** 0.1.0 (workspace)  
**Status:** 🔄 READY FOR CRATES.IO  
**Registry:** crates.io  
**Description:** Core Rust library for VSA

**Publishing:**
- Part of Cargo workspace
- Must be published BEFORE vsa-cli (dependency)
- Command: `cargo publish -p vsa-core`

---

#### 7. `vsa-wasm`
**Location:** `vsa/vsa-wasm/`  
**Current Version:** 0.1.0 (workspace)  
**Status:** 🔄 READY FOR CRATES.IO + NPM  
**Registry:** crates.io + npm (as WASM package)  
**Description:** WASM bindings for Node.js

**Publishing:**
- Can publish to crates.io: `cargo publish -p vsa-wasm`
- Can compile to WASM and publish to npm: `wasm-pack build`

---

#### 8. `eventstore-sdk-rs`
**Location:** `event-store/sdks/sdk-rs/`  
**Current Version:** Unknown  
**Status:** ⚠️ EXPERIMENTAL  
**Registry:** crates.io (if public)  
**Description:** Rust SDK for Event Store

**Decision Needed:**
- Check maturity level
- Decide if public or internal-only

---

### Python Packages

#### 9. `event-sourcing-python`
**Location:** `event-sourcing/python/`  
**Current Version:** 0.1.0  
**Status:** 🔄 READY FOR PYPI  
**Registry:** PyPI  
**Description:** Python SDK for Event Sourcing patterns

**Publishing:**
- ⚠️ Repository URL is placeholder: `https://github.com/yourorg/event-sourcing-platform`
- Build: `python -m build`
- Publish: `twine upload dist/*`

---

#### 10. `eventstore-sdk-py`
**Location:** `event-store/sdks/sdk-py/`  
**Current Version:** 0.2.0  
**Status:** ⚠️ EXPERIMENTAL  
**Registry:** PyPI (if public)  
**Description:** Python SDK for Event Store (experimental)

**Decision Needed:**
- Check maturity level
- Decide if public or internal-only

---

## 🎯 Publishing Strategy

### Phase 1: TypeScript (CURRENT)
**Target:** Q4 2025  
**Packages:**
- ✅ `@event-sourcing-platform/typescript` v0.1.0

**Status:** Ready to publish (tag created, awaiting push)

---

### Phase 2: Rust Crates
**Target:** Q4 2025  
**Packages:**
1. `vsa-core` v0.1.0 (publish first - dependency)
2. `vsa-cli` v0.1.0 (depends on vsa-core)
3. `vsa-wasm` v0.1.0 (optional WASM bindings)
4. `event-sourcing-rust` v0.1.0 (independent)

**Prerequisites:**
- Update repository URLs in Cargo.toml files
- Create crates.io account
- Get API token
- Set up CI/CD for Rust publishing

**Publishing Order:**
```bash
# 1. vsa-core (no dependencies)
cd vsa/vsa-core && cargo publish

# 2. vsa-cli (depends on vsa-core)
cd vsa/vsa-cli && cargo publish

# 3. vsa-wasm (optional)
cd vsa/vsa-wasm && cargo publish

# 4. event-sourcing-rust (independent)
cd event-sourcing/rust && cargo publish
```

---

### Phase 3: Python Packages
**Target:** Q1 2026  
**Packages:**
1. `event-sourcing-python` v0.1.0

**Prerequisites:**
- Update repository URLs in pyproject.toml
- Create PyPI account
- Get API token
- Set up CI/CD for Python publishing

**Publishing:**
```bash
cd event-sourcing/python
python -m build
twine upload dist/*
```

---

### Phase 4: VSCode Extension
**Target:** Q1 2026  
**Packages:**
1. `vsa-vscode` v0.1.0

**Prerequisites:**
- Create VS Marketplace publisher account
- Get Personal Access Token (PAT)
- Verify extension functionality

**Publishing:**
```bash
cd vsa/vscode-extension
vsce publish
```

---

## 🔄 Versioning Strategy

### Independent Versioning (RECOMMENDED)

Each package maintains its own version:

```
@event-sourcing-platform/typescript → v0.1.0 → v0.2.0 → v1.0.0
event-sourcing-rust → v0.1.0 → v0.1.1 → v0.2.0
event-sourcing-python → v0.1.0 → v0.1.0 → v0.1.1
vsa-cli → v0.1.0 → v0.2.0 → v1.0.0
```

**Advantages:**
- ✅ Only bump version when package actually changes
- ✅ Clearer changelog per package
- ✅ More accurate semantic versioning

**Disadvantages:**
- ⚠️ Need to track multiple versions
- ⚠️ Can be confusing if packages are tightly coupled

---

### Synchronized Versioning (ALTERNATIVE)

All packages share the same version:

```
All packages: v0.1.0 → v0.2.0 → v1.0.0
```

**Advantages:**
- ✅ Simple to understand
- ✅ Clear "platform version"
- ✅ Good for marketing

**Disadvantages:**
- ❌ Wasteful (bump all even if one changes)
- ❌ Can mislead users (implies all changed)

---

### RECOMMENDATION: Hybrid Approach

**Major version synchronized, minor/patch independent:**

```
TypeScript: v0.1.0, v0.2.0, v0.2.1, v1.0.0
Rust:       v0.1.0, v0.1.1, v0.1.2, v1.0.0
Python:     v0.1.0, v0.1.0, v0.1.1, v1.0.0
VSA:        v0.1.0, v0.2.0, v0.3.0, v1.0.0
```

- All start at v0.x.x (beta)
- All move to v1.0.0 together (GA)
- Independent minor/patch bumps

---

## 🚀 CI/CD Strategy

### Current Status

#### TypeScript ✅
- ✅ `.github/workflows/test.yml` - Testing on PR/push
- ✅ `.github/workflows/publish.yml` - Publish on release
- ✅ `.github/workflows/codeql.yml` - Security scanning
- ✅ Dependabot configured

#### Rust ⚠️
- ❌ No CI/CD workflows yet
- Need: `rust-test.yml`, `rust-publish.yml`

#### Python ⚠️
- ❌ No CI/CD workflows yet
- Need: `python-test.yml`, `python-publish.yml`

#### VSCode Extension ⚠️
- ❌ No CI/CD workflows yet
- Need: `vscode-publish.yml`

---

### Proposed Workflow Structure

```
.github/workflows/
├── test.yml              ✅ (TypeScript - done)
├── publish.yml           ✅ (TypeScript - done)
├── codeql.yml            ✅ (Security - done)
├── rust-test.yml         ⚠️ (TODO)
├── rust-publish.yml      ⚠️ (TODO)
├── python-test.yml       ⚠️ (TODO)
├── python-publish.yml    ⚠️ (TODO)
└── vscode-publish.yml    ⚠️ (TODO)
```

---

## 📝 Pre-Publishing Checklist

### For Each Package:

#### TypeScript/Node.js
- [ ] Update repository URL in package.json
- [ ] Verify publishConfig
- [ ] Run tests: `pnpm test`
- [ ] Run linter: `pnpm lint`
- [ ] Build: `pnpm build`
- [ ] Update CHANGELOG.md
- [ ] Bump version: `npm version [major|minor|patch]`
- [ ] Create GitHub release
- [ ] CI/CD publishes automatically

#### Rust
- [ ] Update repository URL in Cargo.toml
- [ ] Run tests: `cargo test`
- [ ] Run clippy: `cargo clippy`
- [ ] Dry-run: `cargo publish --dry-run`
- [ ] Update CHANGELOG.md
- [ ] Publish: `cargo publish`

#### Python
- [ ] Update repository URL in pyproject.toml
- [ ] Run tests: `pytest`
- [ ] Run linter: `ruff check`
- [ ] Run type check: `mypy`
- [ ] Build: `python -m build`
- [ ] Update CHANGELOG.md
- [ ] Publish: `twine upload dist/*`

#### VSCode Extension
- [ ] Update version in package.json
- [ ] Run tests: `npm test`
- [ ] Package: `vsce package`
- [ ] Test .vsix locally
- [ ] Update CHANGELOG.md
- [ ] Publish: `vsce publish`

---

## 🔐 Required Accounts & Tokens

### Completed ✅
- [x] GitHub account (for GitHub Packages)
- [x] GitHub Actions enabled

### TODO ⚠️
- [ ] crates.io account + API token (for Rust crates)
- [ ] PyPI account + API token (for Python packages)
- [ ] VS Marketplace publisher account + PAT (for VSCode extension)

---

## 📋 Next Steps

### Immediate (This Week)
1. ✅ Push TypeScript package to GitHub (v0.1.0 tag ready)
2. ⚠️ Verify CI/CD runs successfully
3. ⚠️ Test installation from GitHub Packages

### Short Term (Next 2 Weeks)
4. Update all repository URLs (Rust + Python)
5. Create crates.io account
6. Set up Rust CI/CD workflows
7. Publish Rust crates (vsa-core, vsa-cli, event-sourcing-rust)

### Medium Term (Next Month)
8. Create PyPI account
9. Set up Python CI/CD workflows
10. Publish Python package (event-sourcing-python)

### Long Term (Q1 2026)
11. Create VS Marketplace publisher
12. Publish VSCode extension
13. Consider npm package for vsa-wasm

---

## 🎯 Success Criteria

### TypeScript ✅
- Package available on GitHub Packages
- CI/CD running on every PR
- Auto-publishing on releases
- Installation instructions documented

### Rust (Target)
- All 4 crates on crates.io
- Docs generated and available
- CI/CD for testing
- Installation with `cargo add`

### Python (Target)
- Package on PyPI
- Type stubs included
- Docs on readthedocs
- Installation with `pip install`

### VSCode Extension (Target)
- Published on VS Marketplace
- 5-star rating
- Active installs > 100

---

## 📚 Resources

**TypeScript:**
- [GitHub Packages Docs](https://docs.github.com/en/packages)
- [npm Publishing Guide](https://docs.npmjs.com/packages-and-modules)

**Rust:**
- [crates.io Publishing Guide](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [Cargo Book](https://doc.rust-lang.org/cargo/)

**Python:**
- [PyPI Publishing Guide](https://packaging.python.org/en/latest/tutorials/packaging-projects/)
- [twine Documentation](https://twine.readthedocs.io/)

**VSCode:**
- [Publishing Extensions](https://code.visualstudio.com/api/working-with-extensions/publishing-extension)
- [vsce Documentation](https://github.com/microsoft/vscode-vsce)

---

**Last Updated:** November 5, 2025  
**Maintained By:** Event Sourcing Platform Team

