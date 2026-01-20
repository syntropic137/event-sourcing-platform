# Renovate Bot Quick Start Guide

**For:** Event Sourcing Platform Team  
**Purpose:** Learn how to use Renovate's Dependency Dashboard  
**Time to Read:** 5 minutes

---

## What Changed?

**Before (Dependabot):**
- 20+ PRs cluttering your PR list
- Constantly closing unwanted updates
- Same PRs reappearing weekly

**After (Renovate):**
- 1 issue: "Dependency Dashboard"
- Clean PR list (only approved updates)
- You control when PRs are created

---

## The Dependency Dashboard

### Finding It

1. Go to repository issues
2. Look for issue titled: **"Dependency Dashboard"** (pinned)
3. Bookmark it - you'll check it monthly

### What It Looks Like

```markdown
📊 Dependency Dashboard

🔐 Security Updates (Auto-merged)
✅ protobufjs 7.5.4 → 7.5.5 (auto-merged #123)

📦 Minor/Patch Updates Available
☐ prettier 3.7.3 → 3.7.4
☐ turbo 2.6.1 → 2.6.3
☐ @grpc/grpc-js 1.14.2 → 1.14.3

🚨 Major Updates (Breaking Changes)
☐ @types/node 20.19.25 → 25.0.9
☐ typescript-eslint 6.21.0 → 8.48.1
☐ protobufjs 7.5.4 → 8.0.0

🚫 Blocked/Ignored
❌ ts-proto 1.181.2 → 2.11.0 (blocked per ADR-011)
```

---

## Common Tasks

### Task 1: Approve a Minor Update

**When:** You see an update you want to review

**Steps:**
1. Open Dependency Dashboard issue
2. Find the update (e.g., "prettier 3.7.3 → 3.7.4")
3. Check the checkbox next to it: ☑
4. Renovate creates a PR within minutes
5. Review PR, tests run automatically
6. Merge if green

**That's it!** No more hunting through PR spam.

---

### Task 2: Approve Multiple Related Updates

**When:** You want to update a group (e.g., all TypeScript ecosystem)

**Steps:**
1. Look for "TypeScript ecosystem" group in dashboard
2. Check the group checkbox: ☑
3. Renovate creates ONE PR with all related updates
4. Review grouped PR
5. Merge if tests pass

**Benefit:** Updates related deps together, easier to test.

---

### Task 3: Ignore an Update You Don't Want

**When:** Dashboard shows update you're not interested in

**Steps:**
1. Find the update in dashboard
2. Comment on the issue:
   ```
   @renovate ignore <package-name>
   ```
3. Renovate removes it from dashboard
4. Won't appear again

**Example:**
```
@renovate ignore react
```

---

### Task 4: Check What's Outdated

**When:** Planning a modernization sprint

**Steps:**
1. Open Dependency Dashboard
2. Scroll through all sections
3. Note major updates available
4. Plan which to tackle this quarter

**Benefit:** See everything in one place, plan strategically.

---

### Task 5: Understand Why Something is Blocked

**When:** Dashboard shows "❌ Blocked" next to a dependency

**Steps:**
1. Look for "(blocked per ADR-XXX)" note
2. Read referenced ADR for context
3. Understand why we're not upgrading

**Example:**
```
❌ ts-proto 1.181.2 → 2.11.0 (blocked per ADR-011)
```
→ Read ADR-011 to see why we're staying on v1

---

## What Happens Automatically

### Security Patches (No Action Needed!)

**When Renovate detects a security vulnerability:**

1. ✅ Creates PR immediately (ignores schedule)
2. ✅ Labels it "security" + "high-priority"
3. ✅ Runs CI tests
4. ✅ Auto-merges if tests pass
5. ✅ Notifies you AFTER it's fixed

**You wake up to:** "✅ Security patch merged: protobufjs 7.5.4 → 7.5.5"

**No manual work required!**

---

### Patch Updates (Low Risk)

**For stable dependencies (not v0.x):**

1. Renovate creates PR for patch update
2. Runs CI tests
3. Auto-merges if tests pass
4. Notifies you after merge

**Example:** `prettier 3.7.3 → 3.7.4` (bug fixes only)

**You can disable this** if you prefer manual review for everything.

---

## Monthly Review Process

**Recommended:** Check dashboard first Monday of each month

### Quick Review (10 minutes)

1. Open Dependency Dashboard
2. Scan "Minor/Patch Updates" section
3. Check boxes for safe-looking updates (dev dependencies, build tools)
4. Let Renovate create PRs
5. Merge when green

### Quarterly Deep Dive (1 hour)

1. Review all "Major Updates" section
2. Check changelogs for breaking changes
3. Identify which to tackle this quarter
4. Create milestone for modernization wave
5. Approve updates in batches

---

## Understanding Renovate's Behavior

### When Does Renovate Run?

- **Schedule:** Monday at 3am (weekly check)
- **Security:** Immediately when vulnerability detected
- **Manual:** When you check a box in dashboard

### Why Didn't Renovate Create a PR?

**Possible reasons:**
1. Update requires approval (check the checkbox!)
2. Update is blocked (see `renovate.json` config)
3. Update doesn't match schedule
4. PR limit reached (max 5 concurrent)

### Why Did Renovate Close Its Own PR?

**Possible reasons:**
1. New version available (PR updated to newer version)
2. You unchecked the box in dashboard
3. Configuration changed to block that version

---

## Troubleshooting

### Problem: "Security patch didn't auto-merge"

**Check:**
1. Did CI tests pass?
2. Is branch protection blocking auto-merge?
3. Check Renovate PR for status

**Fix:** Manually merge if needed, investigate config

---

### Problem: "Too many PRs open"

**Check:**
1. Are you approving updates too quickly?
2. Is `prConcurrentLimit` too high?

**Fix:** Let existing PRs merge before approving more

---

### Problem: "I don't see an update I expected"

**Check:**
1. Is it in "Blocked" section?
2. Is it ignored in `renovate.json`?
3. Check Renovate logs in PR comments

**Fix:** Update `renovate.json` to allow it

---

### Problem: "Dashboard is overwhelming"

**Strategy:**
1. Focus on Security section first (auto-handled)
2. Review Minor/Patch monthly
3. Plan Major updates quarterly
4. Ignore what you don't need

**Remember:** You control the pace!

---

## Renovate Commands

Comment these on the Dependency Dashboard issue:

### Ignore a Package
```
@renovate ignore <package-name>
```

### Stop Ignoring a Package
```
@renovate unignore <package-name>
```

### Rebase All Open PRs
```
@renovate rebase
```

### Rebuild Dashboard
```
@renovate rebuild
```

---

## Configuration Changes

### Where: `renovate.json` (repo root)

**Common changes:**

**Block a specific version:**
```json
{
  "packageRules": [
    {
      "matchPackageNames": ["package-name"],
      "allowedVersions": "<2.0.0"
    }
  ]
}
```

**Change auto-merge behavior:**
```json
{
  "packageRules": [
    {
      "matchUpdateTypes": ["patch"],
      "automerge": false  // Disable auto-merge
    }
  ]
}
```

**Add new grouping:**
```json
{
  "packageRules": [
    {
      "groupName": "My Group",
      "matchPackagePatterns": ["^@myorg/"]
    }
  ]
}
```

**After config changes:** Renovate updates dashboard automatically

---

## Getting Help

### Dashboard Issues
- Check issue comments for Renovate status
- Look for error messages or warnings
- Check Renovate logs (in PR comments)

### Configuration Issues
- Validate against schema: https://docs.renovatebot.com/renovate-schema.json
- Read docs: https://docs.renovatebot.com/
- Ask in team channel

### Urgent Security Patch
- If auto-merge failed, merge manually immediately
- Investigate why auto-merge failed later
- Update config to prevent recurrence

---

## Best Practices

### ✅ DO:
- Check dashboard monthly
- Approve updates in small batches
- Read changelogs for major updates
- Keep security section empty (auto-merged)
- Group related updates together
- Document config changes

### ❌ DON'T:
- Approve all major updates at once
- Ignore security patches
- Disable auto-merge for security
- Check every day (creates noise)
- Close Renovate PRs without reading
- Change config without testing

---

## Quick Reference Card

| What You Want | What To Do |
|---|---|
| Approve an update | Check box in dashboard |
| See what's outdated | Open Dependency Dashboard |
| Stop seeing an update | Comment `@renovate ignore <name>` |
| Group related updates | Check group checkbox |
| Handle security patch | Nothing! Auto-merges |
| Block a version | Edit `renovate.json` |
| Create PR now | Check box in dashboard |
| Understand a block | Read referenced ADR |

---

## Team Workflow

### Developer (Daily)
- ✅ Ignore Renovate (it handles security automatically)
- ✅ Review/merge Renovate PRs if green
- ✅ Comment if PR looks suspicious

### Tech Lead (Monthly)
- ✅ Review Dependency Dashboard
- ✅ Approve minor/patch updates
- ✅ Plan major updates for quarter

### DevOps (Quarterly)
- ✅ Deep dive on major updates
- ✅ Plan modernization waves
- ✅ Update ADRs for major decisions

---

## Comparison Cheat Sheet

| Task | Dependabot | Renovate |
|---|---|---|
| See outdated deps | Check each PR manually | Open dashboard issue |
| Approve security patch | Merge PR | Automatic |
| Approve minor update | Wait for PR, then merge | Check box, then merge PR |
| Block major version | Close PR (reappears weekly) | Configure once, never see again |
| Group related updates | Not possible | Check group box |
| See what's available | Count open PRs | Read dashboard |

---

## Key Takeaway

**Renovate gives you control.**

- Security patches: Automatic
- Minor updates: Your schedule
- Major updates: Your decision
- No more PR spam

**Check dashboard monthly, ignore it otherwise.**

---

## Resources

- **Dependency Dashboard:** (link to issue once created)
- **Configuration:** [`renovate.json`](./renovate.json)
- **Documentation:** https://docs.renovatebot.com/
- **ADR:** [ADR-018: Renovate Bot Adoption](./docs/adrs/ADR-018-renovate-bot-adoption.md)
- **Project Plan:** [Renovate Migration Plan](./PROJECT-PLAN_20260120_renovate-migration-and-modernization.md)

---

**Questions?** Ask in team channel or tag DevOps.

**Last Updated:** 2026-01-20

