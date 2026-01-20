# ADR-018: Renovate Bot Adoption for Dependency Management

**Status:** 📋 Proposed  
**Date:** 2026-01-20  
**Decision Makers:** Architecture Team  
**Related:** Dependency Management, Developer Experience, Security

---

## Context

### Current Situation (January 2026)

We are currently using GitHub's Dependabot for automated dependency updates. However, this has created significant operational friction:

**Pain Points:**
- **PR Spam:** 20+ open Dependabot PRs at any given time
- **Repetitive PRs:** Same dependency updates repeatedly created (e.g., ts-proto v2 has 5 PRs)
- **Poor Configuration:** Only 1 of 5 npm ecosystems blocks major version updates
- **Manual Overhead:** ~30-60 minutes/week reviewing and closing unwanted PRs
- **ADR Violations:** Dependabot creates PRs contradicting documented decisions (ADR-011)
- **Noise vs Signal:** Security patches buried among version churn
- **Monorepo Challenges:** Multiple PRs for same dependency across different packages

### Statistics from Analysis
- **20 open PRs** currently (mostly major version updates we don't want)
- **56 closed PRs** in last 100 (0 merged - indicating rejection rate)
- **16 PRs** just for @types/node major version jumps
- **5 PRs** for ts-proto v2 (despite ADR-011 saying to stay on v1)

### Key Requirements

1. **Security First:** Automatic security patch deployment is critical
2. **Reduced Noise:** Stop PR spam from unwanted version updates
3. **Visibility:** Need to see what's available without being overwhelmed
4. **Control:** Manual approval for major/breaking changes
5. **Monorepo Support:** Better handling of related updates across packages
6. **AI Knowledge Gap:** Easier to track and modernize outdated dependencies

---

## Decision

**We will migrate from Dependabot to Renovate Bot for dependency version updates.**

### Key Configuration Decisions

1. **Dependency Dashboard Model:**
   - Single GitHub issue shows all available updates
   - Manual PR creation via checkbox approval
   - Eliminates PR list pollution

2. **Security Patches:**
   - Auto-merge patch updates after CI passes
   - No manual intervention required
   - Overrides normal schedule for immediate deployment

3. **Major Version Handling:**
   - Show in dashboard but don't create PRs
   - Require explicit approval via checkbox
   - Prevents unwanted breaking change PRs

4. **Grouping Strategy:**
   - TypeScript ecosystem: Together
   - gRPC ecosystem: Together
   - Testing frameworks: Together
   - React ecosystem: Together
   - Build tools: Together

5. **Scheduling:**
   - Weekly schedule (Monday 3am)
   - Monthly for minor updates (first of month)
   - Immediate for security patches

6. **ADR Enforcement:**
   - ts-proto: Block v2 entirely (per ADR-011)
   - protobufjs: Block v8 until evaluated
   - @types/node: Pin to Node 20 LTS (20.x)

---

## Rationale

### Why Renovate Over Dependabot

1. **Dependency Dashboard:**
   - **Problem:** 20 PRs cluttering PR list
   - **Solution:** Single issue with all updates, create PRs on demand
   - **Impact:** Clean PR list, review updates on YOUR schedule

2. **Superior Grouping:**
   - **Problem:** 5 separate @types/node PRs across different directories
   - **Solution:** One grouped PR for related updates
   - **Impact:** 80% reduction in PR count

3. **Granular Control:**
   - **Problem:** Dependabot's all-or-nothing approach
   - **Solution:** Per-package, per-update-type configuration
   - **Impact:** Auto-merge patches, approve majors, block specific versions

4. **Better Monorepo Support:**
   - **Problem:** Dependabot treats each package.json separately
   - **Solution:** Renovate understands monorepo structure
   - **Impact:** Coordinated updates across workspace

5. **ADR Enforcement:**
   - **Problem:** No way to permanently block ts-proto v2 in Dependabot
   - **Solution:** `allowedVersions` configuration in Renovate
   - **Impact:** No more repeated PRs for blocked versions

### Why Not Stay with Dependabot

**Considered:** "Just configure Dependabot better"

**Rejected Because:**
- Already configured with major version blocking (1 of 5 ecosystems)
- Still getting unwanted PRs
- Can't permanently block specific versions (ts-proto v2)
- Can't group updates effectively in monorepo
- Can't use dashboard model
- Limited auto-merge capabilities

**Attempted fixes that didn't work:**
- Ignoring dependency versions → Creates new PR when version changes
- Closing PRs manually → PR recreated next week
- Adding ignore rules → Still creates PRs for related deps

### Cost-Benefit Analysis

**Costs:**
- Migration effort: 2-3 hours
- Learning new configuration format
- New workflow for team (dashboard usage)
- Slight risk of misconfiguration

**Benefits:**
- Eliminate 30-60 min/week manual PR review
- Automatic security patch deployment
- Better visibility into outdated dependencies
- Easier major version planning
- Improved developer experience
- Prevents AI knowledge gap (can see what's outdated)

**ROI:** ~10-15 hours saved per quarter after 3-hour investment

---

## Consequences

### Positive

1. **Immediate Noise Reduction** ✅
   - PR list no longer cluttered
   - Single dashboard issue for all updates
   - Manual PR creation only when needed

2. **Automatic Security** ✅
   - Security patches auto-merged
   - No manual intervention required
   - Faster response to vulnerabilities

3. **Better Planning** ✅
   - Dashboard shows all outdated dependencies
   - Can plan modernization waves
   - Track progress on updates

4. **ADR Compliance** ✅
   - ts-proto v2 blocked permanently
   - No more contradictory PRs
   - Configuration matches documented decisions

5. **Improved DX** ✅
   - Less time managing PRs
   - More time building features
   - Better mental model (dashboard vs defensive PR closing)

### Negative

1. **Learning Curve** ⚠️
   - Team needs to learn dashboard workflow
   - New configuration format (JSON vs YAML)
   - **Mitigation:** Documentation + team demo

2. **Configuration Complexity** ⚠️
   - More powerful = more complex
   - Risk of misconfiguration
   - **Mitigation:** Start conservative, iterate

3. **Tool Change Risk** ⚠️
   - New tool might have different bugs
   - Less familiar than Dependabot
   - **Mitigation:** Run in parallel initially

4. **Potential Over-Deferral** ⚠️
   - Dashboard model might lead to ignoring updates
   - Updates pile up if not reviewed regularly
   - **Mitigation:** Monthly review cadence

### Neutral

1. **Still Using GitHub Security Alerts**
   - Keep Dependabot Security Alerts enabled (separate feature)
   - Belt-and-suspenders approach
   - No change to security posture

2. **Configuration in Version Control**
   - `renovate.json` vs `.github/dependabot.yml`
   - Both version controlled
   - Similar workflow

---

## Implementation Plan

### Phase 1: Parallel Testing (Day 1-2)
1. Install Renovate GitHub App
2. Create `renovate.json` configuration
3. Observe dashboard creation
4. Keep Dependabot enabled
5. Compare behavior

### Phase 2: Migration (Day 2-3)
1. Validate Renovate configuration
2. Disable Dependabot version updates
3. Keep Dependabot security alerts
4. Close all Dependabot PRs
5. Enable Renovate fully

### Phase 3: Monitoring (Week 1)
1. Monitor dashboard updates
2. Test auto-merge for patches
3. Test manual PR creation
4. Train team on workflow
5. Document any issues

---

## Alternatives Considered

### Alternative 1: Improve Dependabot Configuration

**Approach:** Add major version blocking to all ecosystems, reduce frequency

**Pros:**
- No tool change
- Familiar workflow
- Simpler

**Cons:**
- Still creates PRs for each package separately
- Can't permanently block ts-proto v2
- No dashboard view
- Limited grouping
- Doesn't solve monorepo issues

**Rejected:** Doesn't address root causes of PR spam

### Alternative 2: Manual Dependency Management

**Approach:** Disable Dependabot entirely, update manually quarterly

**Pros:**
- Complete control
- No PR noise
- Updates when ready

**Cons:**
- Easy to forget
- Security patches delayed
- No automation
- High manual effort
- Miss important updates

**Rejected:** Security patches need automation

### Alternative 3: Custom Scripting Solution

**Approach:** Build custom tool to check deps and create PRs selectively

**Pros:**
- Exactly matches needs
- Full control

**Cons:**
- Development time (weeks)
- Maintenance burden
- Reinventing the wheel
- Need to maintain long-term

**Rejected:** Renovate already solves this problem

---

## Risks and Mitigations

### Risk 1: Renovate Misconfiguration
**Impact:** Security patches not auto-merged, or wrong deps updated  
**Probability:** Medium  
**Mitigation:**
- Start with conservative config
- Test in parallel with Dependabot
- Review first few PRs manually
- Document configuration decisions

### Risk 2: Team Adoption
**Impact:** Team doesn't use dashboard, updates pile up  
**Probability:** Low  
**Mitigation:**
- Demo dashboard to team
- Document workflow
- Set monthly review reminder
- Show time savings benefit

### Risk 3: Regression in Security Coverage
**Impact:** Security vulnerability missed  
**Probability:** Very Low  
**Mitigation:**
- Keep GitHub Security Alerts enabled
- Test security patch auto-merge
- Monitor security advisories
- Renovate uses same vulnerability databases

### Risk 4: Renovate Service Availability
**Impact:** Updates stop if Renovate is down  
**Probability:** Very Low  
**Mitigation:**
- Renovate is open-source (can self-host)
- Free tier has good SLA
- Can fall back to Dependabot temporarily
- Can run Renovate CLI manually

---

## Success Metrics

### Week 1 (Immediate)
- ✅ Zero Dependabot version PRs open
- ✅ Renovate dashboard created
- ✅ Dashboard shows all available updates

### Month 1 (Short-term)
- ✅ At least 1 security patch auto-merged (or none needed)
- ✅ PR count reduced by 80%+
- ✅ Time spent on dependency PRs: <15 min/week
- ✅ Team comfortable with dashboard workflow

### Quarter 1 (Long-term)
- ✅ No regression in security posture
- ✅ Major version updates planned from dashboard
- ✅ Developer satisfaction improved
- ✅ AI knowledge gap reduced (modernization planned)

---

## Rollback Plan

If Renovate doesn't work out:

1. **Disable Renovate:**
   - Remove Renovate GitHub App
   - Delete `renovate.json`
   - Close Renovate dashboard issue

2. **Re-enable Dependabot:**
   - Restore `.github/dependabot.yml`
   - Update with improved configuration
   - Re-enable version updates

3. **Document Lessons:**
   - What didn't work?
   - What configuration issues?
   - Create new ADR if reverting

**Rollback Trigger:** If after 1 month:
- Security patches not working reliably
- Dashboard creates more friction than PRs
- Team strongly prefers old workflow

---

## Related ADRs

- [ADR-011](./ADR-011-ts-proto-v1-retention.md): ts-proto v1 Retention (enforced by Renovate config)
- ADR-019: TypeScript Version Alignment Strategy (to be created)

---

## References

- [Renovate Documentation](https://docs.renovatebot.com/)
- [Renovate GitHub App](https://github.com/marketplace/renovate)
- [GitHub Dependabot Documentation](https://docs.github.com/en/code-security/dependabot)
- [Dependency Dashboard Feature](https://docs.renovatebot.com/key-concepts/dashboard/)
- Internal: [DEPENDABOT-ANALYSIS.md](../../DEPENDABOT-ANALYSIS.md)
- Internal: [PROJECT-PLAN_20260120_renovate-migration-and-modernization.md](../../PROJECT-PLAN_20260120_renovate-migration-and-modernization.md)

---

## Approval

**Proposed By:** Architecture Team  
**Date:** 2026-01-20  
**Requires Approval From:** Tech Lead, DevOps Lead

**Approval Status:** 📋 Awaiting Review

---

**Last Updated:** 2026-01-20  
**Supersedes:** None  
**Superseded By:** None


