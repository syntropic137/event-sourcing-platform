#!/usr/bin/env python3
"""Bump version across all Event Sourcing Platform packages.

Usage:
    python scripts/bump_version.py 0.11.0          # Update all tracked files
    python scripts/bump_version.py --check          # Validate all files match
    python scripts/bump_version.py --current        # Print current version

This script updates version declarations across the monorepo:
- Root package.json (source of truth)
- Rust workspace Cargo.toml files (event-store, vsa)
- TypeScript package.json files (SDKs, visualizer, examples)
- Python pyproject.toml files (SDKs)

Version scheme:
- Minor/major bumps: all packages align to the same version
- Patch bumps: all packages bump together (patch can drift above platform
  version if individual packages get hotfixes, but a minor/major realigns)

Excluded from version tracking:
- Test fixtures (vsa-core/tests/fixtures/*)
- docs-site (always 0.0.0)
- VS Code extension (independent marketplace versioning)
- .venv / node_modules
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# ── Cargo.toml workspace roots ──
# Crates within each workspace inherit via `version.workspace = true`
CARGO_TOML_FILES = [
    ROOT / "event-store/Cargo.toml",
    ROOT / "vsa/Cargo.toml",
    ROOT / "event-sourcing/rust/Cargo.toml",
]

# ── package.json files ──
PACKAGE_JSON_FILES = [
    ROOT / "package.json",
    ROOT / "event-store/sdks/sdk-ts/package.json",
    ROOT / "event-sourcing/typescript/package.json",
    ROOT / "vsa/vsa-visualizer/package.json",
    ROOT / "examples/002-simple-aggregate-ts/package.json",
    ROOT / "examples/004-cqrs-patterns-ts/package.json",
    ROOT / "examples/007-ecommerce-complete-ts/package.json",
]

# ── pyproject.toml files ──
PYPROJECT_FILES = [
    ROOT / "event-store/sdks/sdk-py/pyproject.toml",
    ROOT / "event-sourcing/python/pyproject.toml",
]

# Semver: 0.11.0, 1.0.0-beta, 0.12.0-rc.1, etc.
VERSION_RE = re.compile(r"^\d+\.\d+\.\d+(-[a-zA-Z0-9.]+)?$")

# Regex patterns for replacement (match first occurrence only)
CARGO_VERSION_RE = re.compile(r'^(version\s*=\s*")[^"]*(")', re.MULTILINE)
PYPROJECT_VERSION_RE = re.compile(r'^(version\s*=\s*")[^"]*(")', re.MULTILINE)
PACKAGE_JSON_VERSION_RE = re.compile(r'^(\s*"version"\s*:\s*")[^"]*(")', re.MULTILINE)


def read_cargo_version(path: Path) -> str | None:
    text = path.read_text()
    m = re.search(r'^version\s*=\s*"([^"]*)"', text, re.MULTILINE)
    return m.group(1) if m else None


def read_pyproject_version(path: Path) -> str | None:
    text = path.read_text()
    m = re.search(r'^version\s*=\s*"([^"]*)"', text, re.MULTILINE)
    return m.group(1) if m else None


def read_package_json_version(path: Path) -> str | None:
    data = json.loads(path.read_text())
    return data.get("version")


def read_all_versions() -> dict[Path, str | None]:
    versions: dict[Path, str | None] = {}
    for p in CARGO_TOML_FILES:
        versions[p] = read_cargo_version(p)
    for p in PACKAGE_JSON_FILES:
        versions[p] = read_package_json_version(p)
    for p in PYPROJECT_FILES:
        versions[p] = read_pyproject_version(p)
    return versions


def get_current_version() -> str:
    """Read version from root package.json (source of truth)."""
    v = read_package_json_version(ROOT / "package.json")
    if not v:
        print("ERROR: Could not read version from root package.json", file=sys.stderr)
        sys.exit(1)
    return v


def check_consistency() -> bool:
    """Validate all tracked files have the same version. Returns True if consistent."""
    versions = read_all_versions()
    unique = set(versions.values())
    total = len(versions)

    if None in unique:
        missing = [str(p.relative_to(ROOT)) for p, v in versions.items() if v is None]
        print(f"ERROR: Could not read version from: {', '.join(missing)}", file=sys.stderr)
        return False

    if len(unique) == 1:
        print(f"OK: All {total} files at v{unique.pop()}")
        return True

    print("ERROR: Version mismatch across files:", file=sys.stderr)
    for path, version in sorted(versions.items(), key=lambda x: str(x[0])):
        rel = path.relative_to(ROOT)
        print(f"  {rel}: {version}", file=sys.stderr)
    return False


def bump(target: str) -> None:
    """Update all tracked files to the target version.

    Pre-validates all files before writing any changes. If any file
    is missing a version field, fails without modifying anything.
    """
    if not VERSION_RE.match(target):
        print(
            f"ERROR: Invalid version '{target}'. Expected semver (e.g., 0.11.0 or 0.11.0-beta.1)",
            file=sys.stderr,
        )
        sys.exit(1)

    current = get_current_version()
    if current == target:
        print(f"Version is already {target} -- nothing to do.")
        return

    print(f"Bumping: {current} -> {target}\n")

    # Phase 1: Pre-validate all files and prepare new contents
    pending: list[tuple[Path, str]] = []
    errors: list[str] = []

    for path in CARGO_TOML_FILES:
        text = path.read_text()
        new_text = CARGO_VERSION_RE.sub(rf"\g<1>{target}\2", text, count=1)
        if new_text == text:
            errors.append(str(path.relative_to(ROOT)))
        else:
            pending.append((path, new_text))

    for path in PACKAGE_JSON_FILES:
        text = path.read_text()
        new_text = PACKAGE_JSON_VERSION_RE.sub(rf"\g<1>{target}\2", text, count=1)
        if new_text == text:
            errors.append(str(path.relative_to(ROOT)))
        else:
            pending.append((path, new_text))

    for path in PYPROJECT_FILES:
        text = path.read_text()
        new_text = PYPROJECT_VERSION_RE.sub(rf"\g<1>{target}\2", text, count=1)
        if new_text == text:
            errors.append(str(path.relative_to(ROOT)))
        else:
            pending.append((path, new_text))

    if errors:
        print(f"ERROR: No version field found in: {', '.join(errors)}", file=sys.stderr)
        print("No files were modified.", file=sys.stderr)
        sys.exit(1)

    # Phase 2: Write all files (only reached if all pre-checks passed)
    for path, new_text in pending:
        path.write_text(new_text)
        print(f"  ✓ {path.relative_to(ROOT)}")

    print(f"\nDone. Updated {len(pending)} files to v{target}.")
    print(f"Next: git add -A && git commit -m 'chore: bump version to v{target}'")


def main() -> None:
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    arg = sys.argv[1]

    if arg == "--check":
        sys.exit(0 if check_consistency() else 1)
    elif arg == "--current":
        print(get_current_version())
    elif arg.startswith("-"):
        print(f"Unknown flag: {arg}", file=sys.stderr)
        print(__doc__)
        sys.exit(1)
    else:
        # Strip leading 'v' if provided (e.g., "v0.11.0" -> "0.11.0")
        target = arg.lstrip("v")
        bump(target)


if __name__ == "__main__":
    main()
