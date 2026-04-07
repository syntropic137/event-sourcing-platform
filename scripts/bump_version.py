#!/usr/bin/env python3
"""Bump version across all Event Sourcing Platform packages.

Usage:
    python scripts/bump_version.py 0.11.0           # Update all tracked files
    python scripts/bump_version.py --check           # Validate version consistency
    python scripts/bump_version.py --check-bumped    # Validate PR version > release version
    python scripts/bump_version.py --current         # Print current version

This script updates version declarations across the monorepo:
- Root package.json (source of truth)
- Rust workspace Cargo.toml files (event-store, vsa)
- TypeScript package.json files (SDKs, visualizer, examples)
- Python pyproject.toml files (SDKs)

Version scheme:
- Minor/major bumps: all packages align to the same version
- Patch bumps: all packages bump together (patch can drift above platform
  version if individual packages get hotfixes, but a minor/major realigns)

Consistency rules:
- Major.minor MUST match across all packages
- Patch MAY be >= the platform (root) patch version
- Example: platform 0.11.0, individual package 0.11.2 is OK (hotfix)
- A minor bump (0.12.0) realigns everything

Excluded from version tracking:
- Test fixtures (vsa-core/tests/fixtures/*)
- docs-site (always 0.0.0)
- VS Code extension (independent marketplace versioning)
- .venv / node_modules
"""

from __future__ import annotations

import json
import re
import subprocess
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


def parse_semver(version: str) -> tuple[int, int, int, str]:
    """Parse version into (major, minor, patch, prerelease)."""
    m = re.match(r"^(\d+)\.(\d+)\.(\d+)(-.*)?$", version)
    if not m:
        return (0, 0, 0, "")
    return (int(m.group(1)), int(m.group(2)), int(m.group(3)), m.group(4) or "")


def version_gt(a: str, b: str) -> bool:
    """Return True if version a is strictly greater than version b.

    Pre-release versions sort before release (0.11.0-beta < 0.11.0).
    """
    pa = parse_semver(a)
    pb = parse_semver(b)
    # Compare major.minor.patch first
    if pa[:3] != pb[:3]:
        return pa[:3] > pb[:3]
    # Same major.minor.patch: release > pre-release, then lexicographic
    if pa[3] and not pb[3]:
        return False  # pre-release < release
    if not pa[3] and pb[3]:
        return True  # release > pre-release
    return pa[3] > pb[3]


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
    """Validate version consistency across all tracked files.

    Rules:
    - Major.minor MUST match across all packages
    - Patch MAY be >= the platform (root) patch version
    - Pre-release suffix MUST match if present
    """
    versions = read_all_versions()
    total = len(versions)

    if None in set(versions.values()):
        missing = [str(p.relative_to(ROOT)) for p, v in versions.items() if v is None]
        print(f"ERROR: Could not read version from: {', '.join(missing)}", file=sys.stderr)
        return False

    platform_version = get_current_version()
    platform_parsed = parse_semver(platform_version)
    platform_major_minor = (platform_parsed[0], platform_parsed[1])
    platform_prerelease = platform_parsed[3]

    errors: list[str] = []
    for path, version in sorted(versions.items(), key=lambda x: str(x[0])):
        assert version is not None
        parsed = parse_semver(version)
        pkg_major_minor = (parsed[0], parsed[1])
        pkg_prerelease = parsed[3]

        if pkg_major_minor != platform_major_minor:
            rel = path.relative_to(ROOT)
            errors.append(
                f"  {rel}: {version} (major.minor {pkg_major_minor[0]}.{pkg_major_minor[1]}"
                f" != platform {platform_major_minor[0]}.{platform_major_minor[1]})"
            )
        elif pkg_prerelease != platform_prerelease:
            rel = path.relative_to(ROOT)
            errors.append(f"  {rel}: {version} (prerelease mismatch vs platform {platform_version})")
        elif parsed[2] < platform_parsed[2]:
            rel = path.relative_to(ROOT)
            errors.append(
                f"  {rel}: {version} (patch {parsed[2]} < platform patch {platform_parsed[2]})"
            )

    if errors:
        print("ERROR: Version inconsistency:", file=sys.stderr)
        print(f"  Platform version: {platform_version}", file=sys.stderr)
        for e in errors:
            print(e, file=sys.stderr)
        return False

    # Check if all exactly match (ideal) or some have higher patches (acceptable)
    unique = set(versions.values())
    if len(unique) == 1:
        print(f"OK: All {total} files at v{unique.pop()}")
    else:
        print(f"OK: All {total} files consistent (major.minor = {platform_major_minor[0]}.{platform_major_minor[1]})")
        for path, version in sorted(versions.items(), key=lambda x: str(x[0])):
            if version != platform_version:
                rel = path.relative_to(ROOT)
                print(f"  {rel}: v{version} (patch ahead of platform v{platform_version})")

    return True


def check_bumped() -> bool:
    """Validate that PR version is strictly greater than the release branch version.

    Used by the release gate CI to ensure version was bumped before merge.
    """
    current = get_current_version()

    # Get the release branch version
    try:
        result = subprocess.run(
            ["git", "show", "origin/release:package.json"],
            capture_output=True,
            text=True,
            cwd=ROOT,
        )
        if result.returncode != 0:
            # No release branch yet — any version is fine
            print(f"OK: No release branch found. Current version: v{current}")
            return True
        release_data = json.loads(result.stdout)
        release_version = release_data.get("version", "0.0.0")
    except (json.JSONDecodeError, FileNotFoundError):
        print("WARNING: Could not read release branch version, skipping bump check")
        return True

    if version_gt(current, release_version):
        print(f"OK: v{current} > v{release_version} (release)")
        return True

    print(f"ERROR: Version not bumped!", file=sys.stderr)
    print(f"  Current:  v{current}", file=sys.stderr)
    print(f"  Release:  v{release_version}", file=sys.stderr)
    print(f"  Run: make bump-version VERSION=<new-version>", file=sys.stderr)
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
    elif arg == "--check-bumped":
        ok = check_consistency()
        if not ok:
            sys.exit(1)
        sys.exit(0 if check_bumped() else 1)
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
