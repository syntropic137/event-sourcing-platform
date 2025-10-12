---
sidebar_label: Docs Maintenance
sidebar_position: 40
slug: /development/docs-maintenance
---

# Docs Maintenance

Keep the doc set lightweight and accurate:

## Regenerate code references

```bash
# TypeScript stubs
make gen-ts

# Python stubs and wheel (pyenv 3.11.8 assumed)
PYENV_VERSION=3.11.8 make gen-py
PYENV_VERSION=3.11.8 python -m build
```

Rust consumers always compile against the local workspace, so no extra step is required.

## Update the site

```bash
cd docs-site
npm install
npm run build
```

Use `npm run start` for live previews.

## Style guide

- Prefer short sections with clear links instead of long prose.
- Keep front-matter labels and `_category_.json` files in sync so the sidebar stays tidy.
- Call out breaking changes in the Event Store index, not in every page.

When in doubt: "as simple as possible, but no simpler." Trim copy ruthlessly and link to source files when deeper detail is needed.
