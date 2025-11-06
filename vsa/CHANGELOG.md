# Changelog

All notable changes to the VSA (Vertical Slice Architecture Manager) project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2025-11-05

### Added

#### Milestone 1: Project Setup & Core Infrastructure ✅

**Core Library (vsa-core)**
- Configuration parsing with YAML support
- File system scanning for contexts and features
- Pattern matching for VSA file types (Command, Event, Handler, Query, Test)
- Basic structure validation
- Manifest generation (JSON/YAML)
- Framework integration support
- Bounded context utilities

**CLI Tool (vsa-cli)**
- `vsa init` - Initialize VSA configuration
- `vsa validate` - Validate VSA structure
- `vsa list` - List contexts and features
- `vsa manifest` - Generate manifest file
- Interactive and non-interactive modes
- Verbose logging support

**Infrastructure**
- Rust workspace with 3 crates (vsa-core, vsa-cli, vsa-wasm)
- GitHub Actions CI/CD pipeline
  - Multi-platform testing (Linux, macOS, Windows)
  - Code coverage with Codecov
  - Security audit with cargo-audit
  - Release automation
- Development tooling
  - Makefile with common tasks
  - rustfmt configuration
  - clippy linting

**Documentation**
- 5 Architecture Decision Records
  - ADR-001: Rust Core with Multi-Language Support
  - ADR-002: Convention Over Configuration
  - ADR-003: Bounded Context Structure
  - ADR-004: Integration Event Single Source
  - ADR-005: Framework Integration Strategy
- Comprehensive README with quick start guide
- Detailed project plan with 6 milestones

**Testing**
- 13 unit tests covering core functionality
- Test coverage for configuration, scanning, patterns, validation

**WASM Support (Placeholder)**
- Basic WASM bindings structure
- Ready for future Node.js/browser integration

### Technical Details

**Lines of Code**: ~1,600 lines of Rust  
**Test Coverage**: 13 passing tests  
**Dependencies**: clap, serde, serde_yaml, handlebars, walkdir, regex, chrono  
**Rust Edition**: 2021  
**Minimum Rust Version**: 1.70+

### Next Steps

See Milestone 2 in PROJECT-PLAN for upcoming features:
- Comprehensive validation rules
- Integration event validation
- Bounded context boundary checks
- Cross-context communication validation

[unreleased]: https://github.com/yourusername/vsa/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/yourusername/vsa/releases/tag/v0.1.0

