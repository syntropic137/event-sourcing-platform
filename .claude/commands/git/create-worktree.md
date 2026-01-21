---
allowed-tools: Bash(git:*), Bash(mkdir:*), Bash(ls:*), Bash(cd:*), Bash(gh:*), Bash(cursor:*)
argument-hint: <ACTION> [WT_NAME] [BRANCH] [BASE]
description: Git worktree manager for parallel development workflows
---

# Git Worktree Manager

Manage worktrees in a sibling directory (`../<repo>_wt/`) to support parallel feature development.

## Parameters

| Param | Required | Description |
|-------|----------|-------------|
| `ACTION` | Yes | `create`, `list`, `status`, `remove`, `open` |
| `WT_NAME` | For create/remove/open | Format: `YYYYMMDD_<slug>` |
| `BRANCH` | No | Branch name (defaults to `WT_NAME`) |
| `BASE` | No | Base branch (defaults to `main`) |

**Argument positions:**
- `$1` → `ACTION`
- `$2` → `WT_NAME`
- `$3` → `BRANCH`
- `$4` → `BASE`

## Initialize Variables

Run first to configure paths:

```bash
REPO=$(basename "$(git rev-parse --show-toplevel)")
ROOT=$(git rev-parse --show-toplevel)
WT_DIR="$(dirname "$ROOT")/${REPO}_wt"

echo "Repository: $REPO"
echo "Worktree directory: $WT_DIR"
```

## Commands

### create

Create worktree with new feature branch:

```bash
# Required: WT_NAME
# Optional: BRANCH (defaults to WT_NAME), BASE (defaults to main)

mkdir -p "$WT_DIR"
git fetch origin

BRANCH="${BRANCH:-$WT_NAME}"
BASE="${BASE:-main}"

git worktree add -b "$BRANCH" "$WT_DIR/$WT_NAME" "origin/$BASE"

echo "Created: $WT_DIR/$WT_NAME"
echo "Branch: $BRANCH (based on $BASE)"
echo "Open: cursor $WT_DIR/$WT_NAME"
```

### list

Show all worktrees:

```bash
git worktree list
```

### status

Display worktree status with PR info:

```bash
git worktree list | while read -r PATH _; do
    echo "======================================="
    echo "Worktree: $(basename "$PATH")"
    
    BR=$(cd "$PATH" && git branch --show-current 2>/dev/null)
    echo "Branch: $BR"
    
    (cd "$PATH" && git status -sb 2>/dev/null)
    
    # Check for associated PR
    if command -v gh &>/dev/null && [ -n "$BR" ]; then
        PR=$(gh pr list --head "$BR" --json number,state --jq '.[0] | "PR #\(.number) [\(.state)]"' 2>/dev/null)
        echo "PR: ${PR:-none}"
    fi
    echo ""
done
```

### remove

Delete a worktree:

```bash
# Required: WT_NAME

git worktree remove --force "$WT_DIR/$WT_NAME"
echo "Removed: $WT_NAME"
git worktree prune
```

### open

Launch worktree in editor:

```bash
# Required: WT_NAME

cursor "$WT_DIR/$WT_NAME"
echo "Opened: $WT_DIR/$WT_NAME"
```

## Naming Format

Prefix with date: `YYYYMMDD_<description>`

| Example | Purpose |
|---------|---------|
| `20260115_user-auth` | Feature development |
| `20260118_hotfix-login` | Bug fix |
| `20260120_review-pr-42` | Code review |

## Directory Layout

```
~/projects/
├── myapp/                 <-- main repository
│   └── .git/
└── myapp_wt/              <-- worktrees directory
    ├── 20260115_feature-x/
    └── 20260118_bugfix-y/
```

Advantages:
- Clean separation from main repo
- Independent editor sessions per worktree
- Isolated working contexts
- Prevents accidental file mixing
