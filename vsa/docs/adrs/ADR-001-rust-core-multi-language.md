# ADR-001: Rust Core with Multi-Language Support

**Status:** Accepted  
**Date:** 2025-11-05  
**Deciders:** Architecture Team  
**Context:** Vertical Slice Architecture Manager implementation language

## Context and Problem Statement

The VSA tool needs to:
- Support multiple target languages (TypeScript, Python, Rust)
- Provide fast validation for large codebases
- Offer a CLI tool and programmatic API
- Potentially integrate with build tools and IDEs

Which implementation language should we use for the core tool?

## Decision Drivers

- **Performance**: Validation should be fast enough for watch mode
- **Cross-platform**: Must run on macOS, Linux, Windows
- **Multi-language**: Need to support TypeScript, Python, Rust projects
- **Integration**: Should be usable from Node.js, Python, and CLI
- **Existing Codebase**: Event sourcing platform is primarily Rust
- **Maintenance**: Should be maintainable long-term

## Considered Options

### Option 1: TypeScript-First
Build in TypeScript, port to Python later
- ✅ Leverage existing TS ecosystem
- ✅ Easy integration with Node.js
- ❌ Node.js dependency required
- ❌ Slower than compiled languages
- ❌ Need separate Python implementation

### Option 2: Python-First
Build in Python for maximum accessibility
- ✅ Easy to write and maintain
- ✅ Native Python integration
- ❌ Slower performance
- ❌ Need separate TS/Rust implementations
- ❌ Packaging complexity

### Option 3: Rust Core with Bindings
Build core in Rust, expose via WASM and FFI
- ✅ Excellent performance
- ✅ Single source of truth
- ✅ Cross-platform binary
- ✅ WASM for Node.js/browser
- ✅ Matches existing codebase
- ❌ Higher initial complexity

## Decision Outcome

**Chosen option: Option 3 - Rust Core with Bindings**

### Implementation Strategy

```
vsa-core (Rust)     → Core library (config, validator, scanner)
      ↓
  ┌───┴────┐
  ↓        ↓
vsa-cli   vsa-wasm
(Rust)    (WASM)
  ↓        ↓
CLI       Node.js/Browser
```

### Rationale

1. **Performance**: Rust provides the speed needed for large codebases
2. **Single Implementation**: One core, multiple interfaces
3. **Ecosystem Match**: Event sourcing platform is Rust-based
4. **WASM**: Easy Node.js integration without Node.js dependency
5. **Binary Distribution**: Single executable, no runtime required
6. **Type Safety**: Rust's type system prevents many bugs

### Positive Consequences

- Fast validation (important for watch mode)
- Consistent behavior across languages
- Can be used in CI/CD without Node.js/Python
- Easy to distribute (single binary)
- Strong type safety during development

### Negative Consequences

- Higher barrier to contribution (Rust knowledge required)
- Initial development slower than scripting languages
- WASM bundle size considerations
- Need to maintain bindings

## Mitigation Strategies

- **Contribution Barrier**: Comprehensive documentation, well-structured code
- **Development Speed**: Use existing crates, avoid premature optimization
- **WASM Size**: Profile and optimize, consider lazy loading
- **Bindings**: Keep interface minimal, auto-generate where possible

## Links

- [Rust Book](https://doc.rust-lang.org/book/)
- [wasm-bindgen](https://rustwasm.github.io/wasm-bindgen/)
- Event Sourcing Platform (Rust implementation)

