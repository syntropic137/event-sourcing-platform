# ADR-011: Retain ts-proto v1.172.0

**Status:** ✅ Accepted  
**Date:** 2025-12-02  
**Decision Makers:** Architecture Team  
**Related:** Event Store SDK, Protocol Buffers, TypeScript

## Context

During the December 2025 dependency consolidation (PR #67), we encountered a decision point regarding `ts-proto`, the TypeScript protobuf code generator used in the Event Store SDK.

### Current State

- **Current Version:** `ts-proto@1.172.0`
- **Available Version:** `ts-proto@2.8.3`
- **Usage:** Event Store SDK TypeScript (`/event-store/sdks/sdk-ts`)
- **Purpose:** Generate TypeScript interfaces and serialization code from `.proto` files

### Breaking Changes in ts-proto v2

The v2 release introduced a significant breaking change:

1. **New Dependency:** `@bufbuild/protobuf`
   - v1 used built-in serialization
   - v2 requires external `@bufbuild/protobuf` package
   - Generated code imports: `import { BinaryReader, BinaryWriter } from "@bufbuild/protobuf/wire";`

2. **Migration Scope:**
   - Regenerate all protobuf TypeScript files
   - Add `@bufbuild/protobuf` dependency
   - Update SDK build process
   - Validate compatibility with existing Event Store server
   - Test across all examples and dependent projects

### Why This Matters

The Event Store SDK is a foundational component:
- Used by all TypeScript examples (7+ projects)
- Core to Event Store gRPC communication
- Any breaking change propagates widely
- Requires comprehensive testing

## Decision

**We will retain `ts-proto@1.172.0` and not upgrade to v2 at this time.**

### Rationale

1. **Stability Over Novelty**
   - v1.172.0 is stable and working
   - No known bugs or limitations
   - v2 provides no critical features we need

2. **Migration Risk > Migration Benefit**
   - Breaking changes require extensive testing
   - Risk of regression in Event Store communication
   - Would delay current dependency consolidation work

3. **Deferred Work Is Fine**
   - v2 will continue to mature
   - Can revisit in dedicated SDK update cycle
   - Not blocking any current features

4. **Consolidation Focus**
   - Current PR focuses on safe, incremental updates
   - Breaking changes belong in separate, focused PRs
   - Keep rollback scope manageable

## Consequences

### Positive

1. **Immediate Stability** ✅
   - No risk of breaking Event Store SDK
   - All existing code continues working
   - Dependency consolidation PR stays focused

2. **Deferred Complexity** ✅
   - v2 migration can be planned separately
   - More time to evaluate v2 maturity
   - Can batch with other SDK improvements

3. **Clear Scope** ✅
   - Dependency updates remain safe and incremental
   - Easier to review and test
   - Lower risk of rollback

### Negative

1. **Missed v2 Features** ⚠️
   - Not leveraging latest ts-proto capabilities
   - Potential performance improvements deferred
   - **Mitigation:** v1 is still maintained and functional

2. **Future Migration Debt** ⚠️
   - Will eventually need to migrate
   - Gap between v1 and v2 may grow
   - **Mitigation:** Can tackle in dedicated SDK update PR

### Neutral

1. **Dependabot Will Keep Alerting**
   - Will see continued v2 update PRs
   - Can close with reference to this ADR
   - Revisit decision in Q1 2026

## When to Revisit

Consider upgrading to ts-proto v2 when:

1. **New Features Needed:** v2 provides capabilities we require
2. **v1 Deprecated:** ts-proto v1 no longer maintained
3. **SDK Refactor:** Planned Event Store SDK overhaul
4. **Ecosystem Migration:** Major protobuf ecosystem shift to `@bufbuild`

## Migration Path (Future)

When ready to migrate, follow this approach:

1. **Create Dedicated Branch:** `feat/ts-proto-v2-migration`
2. **Add Dependency:** `pnpm add @bufbuild/protobuf -w`
3. **Update ts-proto:** `pnpm add -D ts-proto@latest -w`
4. **Regenerate Protos:** `cd event-store/sdks/sdk-ts && pnpm run gen`
5. **Test Comprehensively:**
   - SDK unit tests
   - Event Store integration tests
   - All example projects
   - gRPC communication end-to-end
6. **Document Changes:** Update SDK README with new dependency
7. **Staged Rollout:** Test in examples before production

## Related ADRs

- None (first protobuf-related ADR)

## References

- [ts-proto GitHub](https://github.com/stephenh/ts-proto)
- [ts-proto v2 Release Notes](https://github.com/stephenh/ts-proto/releases/tag/v2.0.0)
- [Buf Build Protobuf](https://github.com/bufbuild/protobuf-es)
- PR #67: Consolidated Dependency Updates

---

**Last Updated:** 2025-12-02  
**Supersedes:** None  
**Superseded By:** None

