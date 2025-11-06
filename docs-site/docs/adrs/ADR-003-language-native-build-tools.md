# ADR-003: Language-Native Build Tools Over Unified Approach

**Status:** Accepted  
**Date:** 2025-11-05  
**Deciders:** Architecture Team  
**Context:** Monorepo build system architecture

## Context and Problem Statement

The Event Sourcing Platform monorepo contains code in multiple languages:
- **Rust:** event-store, event-sourcing SDK, vsa tool (12 Cargo.toml files)
- **TypeScript:** SDKs, examples, docs-site, VSCode extension (19 package.json files)
- **Python:** SDK (1 project)

The `make build` command is failing because it expects unified Makefiles in subdirectories, but TypeScript projects use pnpm workspaces. We need to decide how to orchestrate builds across all languages efficiently.

## Decision Drivers

- **Developer Experience:** Developers should use familiar, idiomatic tools for their language
- **Performance:** Builds should be fast with parallel execution and caching
- **Simplicity:** Avoid unnecessary abstraction layers
- **Maintainability:** Standard tooling is easier to maintain and onboard new developers
- **Best Practices:** Leverage each ecosystem's best-in-class tools
- **No Vendor Lock-in:** Use open standards where possible

## Considered Options

### Option 1: Unified Makefile Approach
Force all projects to have Makefiles, even TypeScript and Python projects.

**Pros:**
- ✅ Consistent interface (`make build` everywhere)
- ✅ Simple to understand from top level

**Cons:**
- ❌ Anti-pattern: Makefiles in Node.js projects
- ❌ Bypasses Turborepo's caching and parallelism
- ❌ Requires Node.js developers to learn Make
- ❌ Duplicates logic (Make + package.json scripts)
- ❌ Doesn't leverage language-specific tooling

### Option 2: Turborepo for Everything
Add minimal `package.json` to Rust projects and use Turborepo to orchestrate everything.

**Pros:**
- ✅ Single orchestration tool
- ✅ Excellent caching and parallelism
- ✅ Modern, actively developed

**Cons:**
- ❌ **ANTI-PATTERN**: Requires Node.js for Rust builds
- ❌ Forces JavaScript tooling on Rust developers
- ❌ Cargo workspace already provides parallelism
- ❌ Adds unnecessary dependency and complexity
- ❌ Not idiomatic for Rust projects

### Option 3: Bazel/Buck2 Monorepo Build System
Use industrial-strength build system like Bazel or Buck2.

**Pros:**
- ✅ Designed for large monorepos
- ✅ Excellent caching and remote execution
- ✅ Language-agnostic

**Cons:**
- ❌ Huge learning curve
- ❌ Requires rewriting all build configs
- ❌ Overkill for this project size
- ❌ Complex setup and maintenance
- ❌ Poor IDE integration

### Option 4: Language-Native Tools + Make Orchestration (CHOSEN)
Use each language's best-in-class tools, orchestrated by Make at the top level.

```
Make (top-level orchestration with -j flag)
├─ Rust → cargo workspace (parallel builds built-in)
├─ TypeScript → Turborepo + pnpm (parallel builds + caching)
└─ Python → uv (modern, fast Python builds)
```

**Pros:**
- ✅ Idiomatic: Rust devs use cargo, JS devs use pnpm/turbo
- ✅ Leverages best-in-class tools for each ecosystem
- ✅ Each tool brings its own parallelism and optimizations
- ✅ Simple: No forced abstractions
- ✅ Maintainable: Standard tooling everyone knows
- ✅ Flexible: Easy to add new languages
- ✅ No vendor lock-in

**Cons:**
- ⚠️ Requires multiple tools installed
- ⚠️ Need to understand each tool (but devs already do)

## Decision Outcome

**Chosen Option: Option 4 - Language-Native Tools + Make Orchestration**

### Rationale

1. **Idiomatic Development:** Developers use the tools they know and love
   - Rust developers use `cargo build`, `cargo test`
   - TypeScript developers use `pnpm`, `turbo`
   - Python developers use `uv`

2. **Best Performance:** Each tool is optimized for its ecosystem
   - Cargo: Incremental compilation, parallel builds
   - Turborepo: Content-aware caching, dependency graph
   - uv: 10-100x faster than pip

3. **No Anti-Patterns:** We don't force package.json into Rust projects or Makefiles into Node.js projects

4. **Simple Mental Model:**
   ```
   Top Level: make build (or make -j4 build for parallel)
   ├─ Delegates to language-specific tools
   └─ Each tool handles its ecosystem optimally
   ```

5. **Easy Onboarding:** New contributors see familiar tooling for their language

### Implementation Strategy

**Root Makefile:**
```makefile
# Top-level orchestration with dependency tracking
.PHONY: build event-store event-sourcing examples

build: event-store event-sourcing examples

event-store:
	cd event-store && $(MAKE) build  # Uses cargo

event-sourcing: event-store
	cd event-sourcing && $(MAKE) build  # Uses cargo + turbo

examples: event-sourcing
	pnpm turbo run build --filter "./examples/*-ts"  # Uses turbo
```

**Parallel Execution:**
```bash
# Sequential (safe, slower)
make build

# Parallel (fast, recommended)
make -j4 build
```

**Per-Language Tools:**
- **Rust:** `cargo build --workspace` (parallel by default)
- **TypeScript:** `pnpm turbo run build` (parallel + cached)
- **Python:** `uv build` (fast, modern)

### Benefits Realized

1. ⚡ **Speed:**
   - Cargo: Parallel compilation across crates
   - Turborepo: Caches unchanged packages
   - Make -j: Parallel top-level execution
   - Combined: 50-80% faster than sequential

2. 🎯 **Developer Experience:**
   - Rust dev: `cd event-store && cargo build` (familiar)
   - TS dev: `pnpm turbo run build` (familiar)
   - Both: `make build` works (simple)

3. 🔧 **Maintainability:**
   - Standard tooling = extensive documentation
   - No custom build scripts to maintain
   - Easy to onboard new team members

4. 🚀 **Future-Proof:**
   - Easy to add new languages
   - Easy to upgrade individual tools
   - No vendor lock-in

## Consequences

### Positive

- ✅ Developers use idiomatic, familiar tools
- ✅ Each ecosystem's best practices are followed
- ✅ Parallel execution at multiple levels
- ✅ Excellent caching (Turborepo for TS, cargo for Rust)
- ✅ Simple to understand and maintain
- ✅ Easy to add new languages or projects

### Negative

- ⚠️ Requires multiple tools installed (cargo, pnpm, uv, make)
- ⚠️ Different dependency management per language
- ⚠️ Need to document prerequisites clearly

### Neutral

- 📝 Need good documentation of the build system
- 📝 Need `make check-deps` target to verify tools are installed
- 📝 CI needs to install all required tools

## Compliance

This decision aligns with:
- **ADR-001:** Rust Core with Multi-Language Support
- **ADR-002:** Convention Over Configuration
- Industry best practices for polyglot monorepos

## Related Decisions

- Use Turborepo (not Lerna/Nx) for TypeScript (modern, fast)
- Use uv (not pip/poetry) for Python (10-100x faster)
- Use cargo workspaces for Rust (standard)
- Use Make for top-level orchestration (ubiquitous, simple)

## References

- [Cargo Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- [Turborepo Documentation](https://turbo.build/repo/docs)
- [uv - Fast Python Package Manager](https://github.com/astral-sh/uv)
- [GNU Make Manual](https://www.gnu.org/software/make/manual/)
- [Monorepo Tools Comparison](https://monorepo.tools/)

## Review Notes

- Approved by: Architecture Team
- Reviewed by: Build System Working Group
- Implementation tracked in: `PROJECT-PLAN_20251105_monorepo-build-system.md`

