# Known Issues and Enhancements

This document tracks known issues, technical debt, and planned enhancements for the Event Sourcing Platform.

## 🚀 Enhancements

### High Priority

- [ ] **Upgrade to Rust 2024 Edition** - Update all Cargo.toml files to use Rust 2024 edition
  - **Issue**: Currently using 2021 edition which is outdated for 2025
  - **Blockers**: 
    - 2024 edition makes `gen` a reserved keyword, breaking `eventstore_proto::gen` module
    - Requires updating all references to use `r#gen` syntax
    - Need to audit for other potential breaking changes
  - **Files Affected**: All `Cargo.toml` files, `eventstore-proto/src/lib.rs`, and any references to the `gen` module
  - **Estimated Effort**: Medium (half-day refactoring)

### Medium Priority

- [ ] **Complete TypeScript SDK Implementation** - Finish TypeScript SDK with full abstractions
  - **Status**: Package.json created, need core abstractions and types
  - **Dependencies**: Event store gRPC client integration

- [ ] **Complete Rust SDK Implementation** - Finish implementing core components
  - **Status**: Foundation created, multiple TODOs to address
  - **Dependencies**: Event store gRPC client integration

- [ ] **Complete Python SDK Implementation** - Create Python SDK with full abstractions
  - **Status**: Not started
  - **Dependencies**: Event store gRPC client integration

- [ ] **Add Examples Directory** - Create progressive learning examples
  - **Status**: Directory structure planned but not created
  - **Content**: Order management, inventory, banking examples

### Low Priority

- [ ] **Add Integration Tests** - Create comprehensive integration test suite
- [ ] **Add Development Tools** - CLI tools for development and debugging
- [ ] **Performance Benchmarks** - Add benchmarking suite for performance regression testing

## 🐛 Known Issues

### Critical

- None currently identified

### Non-Critical

- **Build warnings**: Some dependencies may have minor version conflicts (to be reviewed)

## 📋 Technical Debt

### Event Sourcing Rust SDK TODOs

The following TODOs need to be addressed in the event-sourcing Rust SDK:

- [ ] **Event Store Client Integration** (`event-sourcing/rust/src/client.rs`)
  - Add tonic gRPC client implementation
  - Initialize client with proper connection management
  - **Lines**: 5, 11

- [ ] **UUID Generation Enhancement** (`event-sourcing/rust/src/event.rs`)
  - Upgrade to UUID v7 with timestamp when available
  - **Line**: 86

- [ ] **Repository Implementation** (`event-sourcing/rust/src/repository.rs`)
  - Implement loading aggregates from event store
  - Implement saving aggregates to event store  
  - Implement existence checking
  - **Lines**: 50, 55, 60

- [ ] **Projection Management** (`event-sourcing/rust/src/projection.rs`)
  - Add projection tracking and management functionality
  - **Line**: 23

### Code Quality

- [ ] **Add comprehensive documentation** - All modules need better rustdoc/JSDoc coverage
- [ ] **Add error handling best practices** - Standardize error types across SDKs
- [ ] **Add logging/tracing** - Implement structured logging for debugging

### Testing

- [ ] **Unit test coverage** - Add unit tests for all core abstractions
- [ ] **Property-based testing** - Add property tests for critical invariants
- [ ] **Load testing** - Test performance under various loads

### DevOps

- [ ] **CI/CD Pipeline** - Add GitHub Actions for automated testing and releases
- [ ] **Docker Images** - Create development and production Docker images
- [ ] **Release Automation** - Automate SDK package publishing

## 🔧 Maintenance

### Dependencies

- [ ] **Regular dependency updates** - Schedule monthly dependency audits
- [ ] **Security audits** - Regular security scanning of dependencies

### Documentation

- [ ] **API Reference** - Generate and maintain API documentation
- [ ] **User Guides** - Create getting started and tutorial documentation
- [ ] **Architecture Documentation** - Document design decisions and patterns

---

## Contributing

When adding new issues:

1. **Use clear, descriptive titles**
2. **Include context and background**
3. **Estimate effort where possible**
4. **Tag with appropriate priority**
5. **Reference related files/modules**

For tracking purposes, move items to "In Progress" when work begins and "Done" when completed.
