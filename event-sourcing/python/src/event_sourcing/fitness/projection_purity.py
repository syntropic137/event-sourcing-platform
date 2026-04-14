"""
Whitelist-based projection purity check.

Projections must be read-only: replaying the entire event store through
a projection must yield the same result with zero external calls. This
check enforces purity by **whitelisting** allowed imports rather than
blacklisting dangerous ones.

A blacklist is fragile -- new side-effecting libraries slip through
undetected. A whitelist is durable: any import not explicitly allowed
is flagged, regardless of what it is. Same principle as Content Security
Policy (CSP) in web security: default-deny, explicit allow.

Imports inside ``if TYPE_CHECKING:`` blocks are always allowed because
they have zero runtime effect.

Usage::

    from event_sourcing.fitness import check_projection_purity

    # With default allowed prefixes only (stdlib + event_sourcing)
    violations = check_projection_purity(Path("my_projection.py"))

    # With project-specific additions
    violations = check_projection_purity(
        Path("my_projection.py"),
        allowed_prefixes={"my_domain.contexts", "my_shared"},
    )
"""

from __future__ import annotations

import ast
from typing import TYPE_CHECKING

from event_sourcing.fitness.violations import Violation

if TYPE_CHECKING:
    from pathlib import Path

# Default allowed module prefixes for projections.
# These are pure stdlib modules and the ESP framework itself.
# Projects extend this with their own domain/shared prefixes.
PROJECTION_ALLOWED_PREFIXES: frozenset[str] = frozenset(
    {
        # Python stdlib (pure, no side effects)
        "__future__",
        "abc",
        "collections",
        "dataclasses",
        "datetime",
        "decimal",
        "enum",
        "functools",
        "logging",
        "math",
        "operator",
        "re",
        "typing",
        "typing_extensions",
        "uuid",
        # ESP framework itself
        "event_sourcing",
    }
)


def check_projection_purity(
    file_path: Path,
    allowed_prefixes: set[str] | None = None,
) -> list[Violation]:
    """Check that a projection file only imports from allowed modules.

    Uses a whitelist approach: any runtime import whose top-level module
    is not in the allowed set is a violation. Imports inside
    ``if TYPE_CHECKING:`` blocks are always allowed.

    Args:
        file_path: Path to the Python projection file.
        allowed_prefixes: Additional module prefixes to allow (merged
            with ``PROJECTION_ALLOWED_PREFIXES``). For example,
            ``{"syn_domain.contexts", "syn_shared"}`` to allow domain
            imports in a Syntropic137 project.

    Returns:
        List of violations. Empty means the projection is pure.
    """
    source = file_path.read_text(encoding="utf-8")
    try:
        tree = ast.parse(source, filename=str(file_path))
    except SyntaxError:
        return [
            Violation(
                file_path=str(file_path),
                line_number=0,
                rule="projection-purity",
                message="Could not parse file (SyntaxError)",
            )
        ]

    all_allowed = set(PROJECTION_ALLOWED_PREFIXES)
    if allowed_prefixes:
        all_allowed.update(allowed_prefixes)

    violations: list[Violation] = []
    type_checking_lines = _find_type_checking_lines(tree)

    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                if _in_type_checking_block(node.lineno, type_checking_lines):
                    continue
                if not _is_allowed(alias.name, all_allowed):
                    violations.append(
                        Violation(
                            file_path=str(file_path),
                            line_number=node.lineno,
                            rule="projection-purity",
                            message=(
                                f"Runtime import '{alias.name}' is not in the "
                                f"allowed whitelist for projections"
                            ),
                        )
                    )

        elif isinstance(node, ast.ImportFrom):
            if node.module is None:
                continue
            # Relative imports (level > 0) are intra-package and always allowed
            if node.level and node.level > 0:
                continue
            if _in_type_checking_block(node.lineno, type_checking_lines):
                continue
            if not _is_allowed(node.module, all_allowed):
                violations.append(
                    Violation(
                        file_path=str(file_path),
                        line_number=node.lineno,
                        rule="projection-purity",
                        message=(
                            f"Runtime import from '{node.module}' is not in the "
                            f"allowed whitelist for projections"
                        ),
                    )
                )

    return violations


def _is_allowed(module_name: str, allowed: set[str]) -> bool:
    """Check if a module name matches any allowed prefix.

    Matches if the module name equals a prefix or starts with it
    followed by a dot. For example, prefix "event_sourcing" matches
    "event_sourcing", "event_sourcing.core", "event_sourcing.core.checkpoint".
    """
    for prefix in allowed:
        if module_name == prefix or module_name.startswith(prefix + "."):
            return True
    return False


def _find_type_checking_lines(tree: ast.Module) -> list[tuple[int, int]]:
    """Find line ranges of ``if TYPE_CHECKING:`` blocks.

    Returns a list of (start_line, end_line) tuples for each
    TYPE_CHECKING guarded block.
    """
    ranges: list[tuple[int, int]] = []

    for node in ast.walk(tree):
        if not isinstance(node, ast.If):
            continue

        # Match: if TYPE_CHECKING:
        test = node.test
        is_type_checking = False

        if isinstance(test, ast.Name) and test.id == "TYPE_CHECKING":
            is_type_checking = True
        elif isinstance(test, ast.Attribute) and test.attr == "TYPE_CHECKING":
            is_type_checking = True

        if is_type_checking:
            start = node.lineno
            # Find the last line in the body
            end = start
            for child in ast.walk(node):
                child_lineno = getattr(child, "lineno", None)
                if child_lineno is not None:
                    end = max(end, child_lineno)
            ranges.append((start, end))

    return ranges


def _in_type_checking_block(lineno: int, ranges: list[tuple[int, int]]) -> bool:
    """Check if a line number falls within any TYPE_CHECKING block."""
    return any(start <= lineno <= end for start, end in ranges)
