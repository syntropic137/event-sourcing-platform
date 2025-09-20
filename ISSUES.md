# Known Issues and Enhancements

This document tracks known issues, technical debt, and planned enhancements for the Event Sourcing Platform.

## 🎉 Current Status (v0.2.0-beta)

**✅ MAJOR ACHIEVEMENT: Complete Event Sourcing Platform Delivered!**

### What's Working
- **✅ Full gRPC Event Store** with PostgreSQL persistence
- **✅ Complete TypeScript SDK** with all core abstractions
- **✅ 9 Progressive Examples** from basic to advanced patterns
- **✅ Live Web Dashboard** making the system observable
- **✅ Development Infrastructure** with Docker containers
- **✅ End-to-End Persistence** - events stored in PostgreSQL database
- **✅ QA Pipeline** - all tests passing

### Examples Implemented
1. **001-basic-store-ts** - Raw event store usage
2. **002-simple-aggregate-ts** - Aggregates with decorators & optimistic concurrency  
3. **003-multiple-aggregates-ts** - Multiple aggregates working together
4. **004-cqrs-patterns-ts** - Command/Query separation with read models
5. **005-projections-ts** - Event-driven projections and analytics
6. **006-event-bus-ts** - Cross-aggregate communication via events
7. **007-inventory-complete-ts** - Complete inventory management system
8. **008-observability-ts** - System monitoring and health metrics
9. **009-web-dashboard-ts** - Live HTML dashboard showing projections

### Infrastructure Status
- **PostgreSQL**: ✅ Running on port 15648 with 18+ events stored
- **Redis**: ✅ Running on port 16648  
- **gRPC Event Store**: ✅ Running on port 50051
- **Web Dashboard**: ✅ Running on port 3000

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

- [x] **Complete TypeScript SDK Implementation** - ✅ COMPLETED
  - **Status**: Full implementation with aggregates, repositories, projections, event bus
  - **Achievement**: 9 working examples from basic to advanced patterns

- [ ] **Complete Rust SDK Implementation** - Finish implementing core components
  - **Status**: Foundation created, multiple TODOs to address (see Technical Debt section)
  - **Dependencies**: Event store gRPC client integration

- [ ] **Complete Python SDK Implementation** - Create Python SDK with full abstractions
  - **Status**: Not started
  - **Dependencies**: Event store gRPC client integration

- [x] **Add Examples Directory** - ✅ COMPLETED
  - **Status**: 9 comprehensive examples implemented and tested
  - **Content**: Basic store, aggregates, CQRS, projections, event bus, inventory, observability, web dashboard

### Low Priority

- [ ] **Add Integration Tests** - Create comprehensive integration test suite
- [ ] **Add Development Tools** - CLI tools for development and debugging  
- [ ] **Performance Benchmarks** - Add benchmarking suite for performance regression testing
- [ ] **Dashboard Enhancements** - Add more visualization features to web dashboard
  - Real-time charts and graphs
  - Event stream visualization
  - Performance metrics dashboard
- [ ] **Example Documentation** - Add comprehensive README files for examples 007-009
- [ ] **CI/CD Integration** - Set up automated testing and deployment pipelines

## 🐛 Known Issues

### Critical

- None currently identified

### Non-Critical

- **Build warnings**: Some dependencies may have minor version conflicts (to be reviewed)
- **Coverage instrumentation mismatch**: `cargo llvm-cov report --text` emits "functions have mismatched data" for `eventstore-sdk-rs` async client helpers. Coverage still completes, but we should investigate whether multiple instrumented builds are being merged or if the crate should be excluded from coverage aggregation.
- **Future Rust compatibility**: `sqlx-postgres v0.7.4` contains code that will be rejected by a future version of Rust
  - **Warning**: `warning: the following packages contain code that will be rejected by a future version of Rust: sqlx-postgres v0.7.4`
  - **Action**: Monitor for sqlx updates or consider upgrading to newer version when available
  - **Impact**: Non-blocking, but should be addressed before future Rust version upgrades

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
