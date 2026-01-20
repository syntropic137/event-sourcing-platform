# Dependabot PR Analysis - January 20, 2026

## Executive Summary

**Current State:** 20 open Dependabot PRs, 56 closed PRs (last 100)
**Root Cause:** Missing ignore rules for major version updates across most package ecosystems
**Impact:** Continuous PR churn from unwanted major version updates

---

## Key Findings

### 1. **Inconsistent Major Version Blocking**

Only **ONE** of the 5 npm ecosystems has major version blocking configured:

```yaml
# ✅ PROTECTED: event-sourcing/typescript
- package-ecosystem: "npm"
  directory: "/event-sourcing/typescript"
  ignore:
    - dependency-name: "*"
      update-types: ["version-update:semver-major"]

# ❌ UNPROTECTED: All others
- /event-store/sdks/sdk-ts
- / (root workspace)
- /docs-site
- GitHub Actions
```

### 2. **Recurring Problem PRs**

#### ts-proto v2 (Breaking Changes)
- **5 PRs created** for same major version upgrade (v1 → v2)
- **All closed** without merging
- **ADR-011 exists** documenting decision to stay on v1
- **Still recurring** because no ignore rule in place

Timeline:
- PR #52 (Nov 17) - ts-proto 1.181.2 → 2.8.3 - CLOSED
- PR #57 (Nov 17) - ts-proto 1.181.2 → 2.8.3 - CLOSED  
- PR #104 (Dec 22) - ts-proto 1.181.2 → 2.10.0 - CLOSED
- PR #107 (Dec 29) - ts-proto 1.181.2 → 2.10.1 - CLOSED
- PR #112 (Jan 19) - ts-proto 1.181.2 → 2.11.0 - **OPEN** ⚠️

#### @types/node (Major Version Jumps)
- **16 PRs total** for @types/node updates
- **Multiple major versions** (20 → 22 → 24 → 25)
- **Currently jumping** from 20.x and 22.x to 25.x
- **CI failures** on some PRs (SDK TypeScript tests)

Recent examples:
- PR #113: @types/node 22.18.0 → 25.0.9 (sdk-ts) - OPEN, CI FAILING
- PR #111: @types/node 20.19.13 → 25.0.9 (docs-site) - OPEN
- PR #79: @types/node 20.19.13 → 24.10.1 (multiple) - OPEN

#### Other Major Updates Creating Churn
- **protobufjs** 7.5.4 → 8.0.0 (PR #106)
- **zod** 4.1.13 → 4.2.0 (PR #97)
- **@typescript-eslint/parser** 6.21.0 → 8.48.1 (PR #82)
- **react** 19.2.0 → 19.2.x (multiple PRs)
- **actions/cache** 4 → 5 (PR #93)
- **actions/upload-artifact** 5 → 6 (PR #91)

### 3. **Package Version Inconsistencies**

Different packages use different Node.js type versions:

```
event-sourcing/typescript:     @types/node ^20.19.25
event-store/sdks/sdk-ts:       @types/node ^24.10.1  
docs-site:                     @types/node ^24.10.1
examples/002-simple-aggregate: @types/node ^20.0.0
vsa/vscode-extension:          @types/node ^20.0.0
```

This creates multiple PRs for the same dependency across different directories.

### 4. **PR Closure Patterns**

From last 200 PRs:
- **40 PRs** closed after multiple days (likely manual review/rejection)
- **16 PRs** closed same day (likely auto-superseded by newer versions)
- **0 PRs** merged in recent history

This indicates most PRs are being rejected, not merged.

---

## Root Cause Analysis

### Why This Keeps Happening

1. **Missing Ignore Rules**: Only 1 of 5 npm ecosystems blocks major updates
2. **No Architectural Decision Enforcement**: ADR-011 documents ts-proto v1 retention, but Dependabot isn't configured to respect it
3. **Weekly Schedule**: All ecosystems run weekly, creating constant PR churn
4. **No Version Pinning Strategy**: Packages use caret ranges (^) allowing major updates
5. **Monorepo Complexity**: Multiple package.json files with different version ranges

### Why Manual Reviews Are Exhausting

- **Volume**: 20+ open PRs at any time
- **Repetition**: Same dependencies keep creating new PRs (ts-proto has 5 iterations)
- **False Positives**: Many updates are breaking changes that can't be merged
- **CI Noise**: Some PRs fail CI, requiring investigation

---

## Recommendations

### Immediate Actions (High Priority)

1. **Add Major Version Blocking to All Ecosystems**

```yaml
# Event Store SDK TypeScript
- package-ecosystem: "npm"
  directory: "/event-store/sdks/sdk-ts"
  schedule:
    interval: "weekly"
  labels:
    - "dependencies"
    - "eventstore-sdk"
  ignore:
    - dependency-name: "*"
      update-types: ["version-update:semver-major"]

# Root workspace  
- package-ecosystem: "npm"
  directory: "/"
  schedule:
    interval: "weekly"
  labels:
    - "dependencies"
    - "monorepo"
  ignore:
    - dependency-name: "*"
      update-types: ["version-update:semver-major"]

# Documentation site
- package-ecosystem: "npm"
  directory: "/docs-site"
  schedule:
    interval: "weekly"
  labels:
    - "dependencies"
    - "docs"
  ignore:
    - dependency-name: "*"
      update-types: ["version-update:semver-major"]

# GitHub Actions
- package-ecosystem: "github-actions"
  directory: "/"
  schedule:
    interval: "weekly"
  labels:
    - "dependencies"
    - "github-actions"
  ignore:
    - dependency-name: "*"
      update-types: ["version-update:semver-major"]
```

2. **Add Specific Ignores for Known Breaking Changes**

```yaml
# Event Store SDK TypeScript
- package-ecosystem: "npm"
  directory: "/event-store/sdks/sdk-ts"
  ignore:
    - dependency-name: "*"
      update-types: ["version-update:semver-major"]
    - dependency-name: "ts-proto"
      versions: [">=2.0.0"]  # ADR-011: Retain v1.x
    - dependency-name: "protobufjs"
      versions: [">=8.0.0"]  # Breaking changes in v8
```

3. **Close All Open Major Version PRs**

```bash
# Close ts-proto v2 PR with explanation
gh pr close 112 --comment "Closing per ADR-011: Retaining ts-proto v1.x. Dependabot config updated to prevent future PRs."

# Close @types/node major version PRs
gh pr close 113 --comment "Closing major version update. Updated Dependabot config to only allow minor/patch updates."
gh pr close 111 --comment "Closing major version update. Updated Dependabot config to only allow minor/patch updates."

# Bulk close remaining major version PRs
gh pr list --author "app/dependabot" --json number,title | \
  jq -r '.[] | select(.title | contains("bump")) | .number' | \
  xargs -I {} gh pr close {} --comment "Closing major version update. Updated Dependabot config."
```

### Medium-Term Actions

4. **Reduce Update Frequency**

Consider changing from `weekly` to `monthly` for less critical dependencies:

```yaml
- package-ecosystem: "npm"
  directory: "/docs-site"
  schedule:
    interval: "monthly"  # Changed from weekly
```

5. **Standardize @types/node Versions**

Align all packages to use the same Node.js type definitions:

```json
// All package.json files should use:
"@types/node": "^20.19.x"
```

6. **Enable Auto-Merge for Safe Updates**

Configure auto-merge for patch/minor updates that pass CI:

```yaml
- package-ecosystem: "npm"
  directory: "/event-sourcing/typescript"
  open-pull-requests-limit: 5
  ignore:
    - dependency-name: "*"
      update-types: ["version-update:semver-major"]
```

Then enable GitHub auto-merge rules for Dependabot PRs with passing CI.

### Long-Term Actions

7. **Document Major Version Update Process**

Create `docs/DEPENDENCY-UPGRADE-PROCESS.md`:
- When to accept major version updates
- How to test breaking changes
- ADR creation requirements
- Dependabot configuration updates

8. **Quarterly Major Version Review**

Schedule quarterly reviews to:
- Evaluate major version updates
- Update ADRs
- Plan migration work
- Update Dependabot ignores

9. **Consider Renovate Bot**

Renovate offers more sophisticated configuration:
- Group related updates
- Better monorepo support
- Dependency dashboard
- More granular scheduling

---

## Impact Assessment

### Before Changes
- ✗ 20 open PRs requiring manual review
- ✗ Weekly PR churn (5-10 new PRs/week)
- ✗ Repeated PRs for same dependencies
- ✗ CI failures from breaking changes
- ✗ Time spent reviewing/closing PRs: ~30-60 min/week

### After Changes
- ✓ ~5-8 open PRs (only patch/minor updates)
- ✓ Reduced churn (2-3 new PRs/week)
- ✓ No repeated PRs for blocked versions
- ✓ Fewer CI failures
- ✓ Time spent reviewing: ~10-15 min/week
- ✓ Major updates handled intentionally via ADRs

---

## Current Open PRs Requiring Action

### Should Close (Major Version Updates)
- #112 - ts-proto 1.x → 2.x (ADR-011)
- #113 - @types/node 22.x → 25.x (CI failing)
- #111 - @types/node 20.x → 25.x
- #106 - protobufjs 7.x → 8.x
- #97 - zod 4.1 → 4.2 (major)
- #82 - @typescript-eslint/parser 6.x → 8.x
- #79 - @types/node 20.x → 24.x
- #93 - actions/cache 4 → 5
- #91 - actions/upload-artifact 5 → 6

### Can Review/Merge (Minor/Patch Updates)
- #96 - @grpc/grpc-js 1.14.2 → 1.14.3 ✓
- #95 - @types/node 20.19.13 → 20.19.27 ✓
- #94 - @eslint/js 9.39.1 → 9.39.2 ✓
- #92 - react 19.2.0 → 19.2.3 ✓
- #89 - react-dom 19.2.0 → 19.2.3 ✓
- #88 - @grpc/grpc-js 1.13.4 → 1.14.3 ✓
- #81 - prettier 3.7.3 → 3.7.4 ✓
- #78 - turbo 2.6.1 → 2.6.3 ✓
- #75 - @easyops-cn/docusaurus-search-local 0.52.1 → 0.52.2 ✓
- #74 - prettier 3.7.3 → 3.7.4 ✓
- #80 - react 19.2.0 → 19.2.1 ✓

---

## Next Steps

1. ✅ Review this analysis
2. ⬜ Update `.github/dependabot.yml` with ignore rules
3. ⬜ Close all major version PRs with explanation
4. ⬜ Merge safe minor/patch PRs
5. ⬜ Document process in ADR
6. ⬜ Schedule quarterly major version review

---

## References

- [ADR-011: ts-proto v1 Retention](docs/adrs/ADR-011-ts-proto-v1-retention.md)
- [Dependabot Configuration Options](https://docs.github.com/en/code-security/dependabot/dependabot-version-updates/configuration-options-for-the-dependabot.yml-file)
- [Semantic Versioning](https://semver.org/)

