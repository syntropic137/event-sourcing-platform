# Backwards Compatibility Testing Checklist

**Purpose:** Ensure dependency updates don't break downstream consumers of the Event Sourcing Platform

**Use:** Run this checklist after each dependency update wave before merging to main

---

## Pre-Update Baseline

Before making ANY dependency changes, establish baseline:

### 1. Public API Surface Area
- [ ] Document all exported functions/classes from `@neuralempowerment/event-sourcing-typescript`
- [ ] Document all exported functions/classes from `@eventstore/sdk-ts`
- [ ] Save generated `.d.ts` files for comparison
- [ ] Document event schemas (protobuf definitions)
- [ ] List all example projects and their expected behavior

### 2. Test Coverage Verification
- [ ] All tests passing: `pnpm run test`
- [ ] All builds successful: `pnpm run build`
- [ ] All examples run: Test each example manually
- [ ] No TypeScript errors: `pnpm run build` (type checking)

### 3. Baseline Artifacts
```bash
# Save current type definitions
mkdir -p .baseline/types
cp -r event-sourcing/typescript/dist/*.d.ts .baseline/types/event-sourcing/
cp -r event-store/sdks/sdk-ts/dist/*.d.ts .baseline/types/event-store/

# Save current package.json versions
cp package.json .baseline/
cp event-sourcing/typescript/package.json .baseline/event-sourcing.package.json
cp event-store/sdks/sdk-ts/package.json .baseline/event-store-sdk.package.json

# Save test output
pnpm run test > .baseline/test-output.txt 2>&1
```

---

## Post-Update Validation

After dependency updates, verify backwards compatibility:

### 1. API Surface Comparison

#### TypeScript Type Definitions
```bash
# Compare generated type definitions
diff -r .baseline/types/event-sourcing/ event-sourcing/typescript/dist/

# Expected: No differences in public API signatures
# Acceptable: Internal/private changes only
# Unacceptable: Public function signature changes
```

- [ ] No changes to exported function signatures
- [ ] No changes to exported class public methods
- [ ] No changes to exported interfaces
- [ ] No changes to exported types
- [ ] No removed exports
- [ ] New exports are additive only (not replacing)

#### Event Schemas (Protobuf)
```bash
# Check protobuf definitions haven't changed
diff event-store/eventstore-proto/proto/eventstore.proto .baseline/eventstore.proto

# Regenerate protobuf code
cd event-store && make gen-ts

# Verify generated code is backwards compatible
diff -r .baseline/types/event-store/ event-store/sdks/sdk-ts/dist/
```

- [ ] Protobuf message definitions unchanged
- [ ] Field numbers unchanged (critical for wire format)
- [ ] No removed fields (deprecated is OK)
- [ ] New fields are optional only
- [ ] Generated TypeScript code is compatible

### 2. Build Verification
```bash
# Clean build from scratch
pnpm run clean
pnpm install
pnpm run build
```

- [ ] Root workspace builds successfully
- [ ] `event-sourcing/typescript` builds successfully
- [ ] `event-store/sdks/sdk-ts` builds successfully
- [ ] `docs-site` builds successfully
- [ ] All example projects build successfully
- [ ] No new TypeScript errors
- [ ] No new build warnings (or documented as acceptable)

### 3. Test Suite Validation
```bash
# Run full test suite
pnpm run test

# Compare with baseline
diff .baseline/test-output.txt <(pnpm run test 2>&1)
```

- [ ] All existing tests still pass
- [ ] No tests skipped that weren't before
- [ ] No new test failures
- [ ] Test output is equivalent (or better)
- [ ] Code coverage not decreased

### 4. Example Projects Validation

For each example project:

#### Example: 002-simple-aggregate-ts
```bash
cd examples/002-simple-aggregate-ts
pnpm install
pnpm run build
pnpm run start
```

- [ ] Builds without errors
- [ ] Runs without runtime errors
- [ ] Expected output matches baseline
- [ ] No console warnings/errors

#### Example: 004-cqrs-patterns-ts
```bash
cd examples/004-cqrs-patterns-ts
pnpm install
pnpm run build
pnpm run start
```

- [ ] Builds without errors
- [ ] Runs without runtime errors
- [ ] CQRS patterns work as expected
- [ ] Event handlers execute correctly

#### Example: 007-ecommerce-complete-ts
```bash
cd examples/007-ecommerce-complete-ts
pnpm install
pnpm run build
pnpm run start
```

- [ ] Builds without errors
- [ ] Runs without runtime errors
- [ ] All domain operations work
- [ ] Event sourcing flow works end-to-end

### 5. Event Store Integration Testing

#### Basic Connection Test
```bash
# Start Event Store (if not running)
docker-compose up -d eventstore

# Run SDK test
cd event-store/sdks/sdk-ts
pnpm run example
```

- [ ] gRPC connection establishes successfully
- [ ] Can write events to Event Store
- [ ] Can read events from Event Store
- [ ] Can subscribe to event streams
- [ ] Serialization/deserialization works correctly

#### Event Sourcing SDK Test
```bash
cd event-sourcing/typescript
pnpm run test
```

- [ ] All aggregate tests pass
- [ ] Event application works correctly
- [ ] Snapshot functionality works
- [ ] Projection tests pass
- [ ] Repository pattern works

### 6. Type Safety Validation

Create a test TypeScript file to verify API usage:

```typescript
// test-backwards-compat.ts
import {
  EventSourcedAggregate,
  DomainEvent,
  EventStore,
  Repository
} from '@neuralempowerment/event-sourcing-typescript';

import { EventStoreClient } from '@eventstore/sdk-ts';

// Test 1: Aggregate usage still compiles
class TestAggregate extends EventSourcedAggregate {
  constructor(id: string) {
    super(id);
  }
  
  apply(event: DomainEvent): void {
    // Test implementation
  }
}

// Test 2: Event Store client still works
const client = new EventStoreClient({
  endpoint: 'localhost:2113'
});

// Test 3: Repository pattern still works
const repo = new Repository<TestAggregate>(
  TestAggregate,
  client
);

console.log('Type checking passed!');
```

```bash
npx tsc --noEmit test-backwards-compat.ts
```

- [ ] Test file compiles without errors
- [ ] No type inference issues
- [ ] Generic types work correctly
- [ ] Import paths are valid

---

## Dependency-Specific Checks

### For TypeScript Updates
- [ ] New TypeScript version doesn't break existing code
- [ ] No new strict mode errors (unless intended)
- [ ] Generated declaration files are equivalent
- [ ] `tsconfig.json` settings still valid
- [ ] Build output structure unchanged

### For @types/node Updates
- [ ] Node.js built-in types are compatible
- [ ] No changes to Buffer, Stream, EventEmitter APIs
- [ ] Async/Promise types unchanged
- [ ] File system types unchanged

### For @grpc/grpc-js Updates
- [ ] gRPC connection still works
- [ ] Streaming still works (read/write/bidirectional)
- [ ] Metadata handling unchanged
- [ ] Error types unchanged
- [ ] Credentials/authentication still works

### For Protobuf-related Updates (ts-proto, protobufjs)
- [ ] Generated TypeScript code is compatible
- [ ] Message serialization unchanged (wire format)
- [ ] Message deserialization unchanged
- [ ] Field accessors unchanged
- [ ] Enum values unchanged

### For Zod Updates
- [ ] Schema validation still works
- [ ] Error messages unchanged (or improved)
- [ ] Type inference still works
- [ ] `.parse()` and `.safeParse()` behavior unchanged

### For Testing Framework Updates (Jest, ts-jest)
- [ ] All test files still execute
- [ ] Mocking still works
- [ ] Snapshot testing still works
- [ ] Code coverage reporting works
- [ ] Test configuration still valid

---

## Breaking Change Assessment

If any of the above checks fail, assess the impact:

### Severity Classification

**Critical (Must Fix Before Merge):**
- Public API signature changes
- Event schema changes (protobuf)
- Runtime errors in examples
- Test failures
- Build failures

**High (Should Fix Before Merge):**
- Type inference changes affecting consumers
- New required dependencies
- Configuration changes required
- Performance degradation

**Medium (Document and Consider):**
- New optional features available
- Deprecated APIs (with working alternatives)
- Minor behavior changes
- New warnings

**Low (Acceptable):**
- Internal implementation changes
- Dev dependency updates
- Documentation updates
- Improved error messages

### Mitigation Strategies

If breaking changes are unavoidable:

1. **Adapter Pattern:**
   - Create compatibility layer
   - Keep old API alongside new
   - Deprecate old API gradually

2. **Versioning:**
   - Bump major version of affected package
   - Document breaking changes in CHANGELOG
   - Provide migration guide

3. **Feature Flags:**
   - Make new behavior opt-in
   - Allow gradual migration
   - Provide transition period

4. **Rollback:**
   - Revert problematic dependency update
   - Document issue
   - Create ADR for future consideration

---

## Documentation Requirements

Before merging any dependency updates:

### 1. Update CHANGELOG
```markdown
## [Unreleased]

### Changed
- Updated TypeScript to 5.9.x across all packages
- Updated @types/node to 20.19.x for consistency

### Dependencies
- typescript: 5.6.3 → 5.9.3
- @types/node: 20.19.13 → 20.19.27
- [list other updates]

### Migration Notes
- No breaking changes
- Public APIs unchanged
- Downstream consumers: no action required
```

### 2. Update README (if needed)
- [ ] Update minimum Node.js version requirement
- [ ] Update TypeScript version requirement
- [ ] Update installation instructions
- [ ] Update example code (if syntax changed)

### 3. Create ADR (for major decisions)
- [ ] Document decision to upgrade (or defer)
- [ ] Rationale for timing
- [ ] Breaking changes assessment
- [ ] Migration strategy

---

## Sign-off Checklist

Before merging dependency updates to main:

- [ ] All automated tests pass
- [ ] All manual tests completed
- [ ] API surface comparison shows no breaking changes
- [ ] All examples run successfully
- [ ] Event Store integration verified
- [ ] Type definitions validated
- [ ] Documentation updated
- [ ] CHANGELOG updated
- [ ] Code review completed
- [ ] QA checkpoint passed

**Sign-off:**
- Developer: _________________ Date: _______
- Reviewer: _________________ Date: _______
- QA: _______________________ Date: _______

---

## Rollback Procedure

If issues are discovered after merge:

1. **Immediate Rollback:**
   ```bash
   # Revert the merge commit
   git revert <merge-commit-sha>
   git push origin main
   ```

2. **Notify Stakeholders:**
   - Post in team channel
   - Update issue/PR with details
   - Document what went wrong

3. **Root Cause Analysis:**
   - What check was missed?
   - How can we prevent this?
   - Update this checklist

4. **Fix and Re-attempt:**
   - Fix issue in new branch
   - Run full checklist again
   - Merge when validated

---

## Continuous Monitoring

After merge, monitor for 48 hours:

- [ ] CI/CD pipelines still green
- [ ] No new issues reported
- [ ] No performance degradation
- [ ] No downstream consumer complaints
- [ ] Dependency dashboard shows expected state

---

**Checklist Version:** 1.0  
**Last Updated:** 2026-01-20  
**Next Review:** After first dependency wave completion

