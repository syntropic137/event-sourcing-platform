"""
Built-in architectural fitness functions for event-sourced systems.

This module ships with ESP so that any project built on the platform can
validate architectural invariants in CI with minimal setup.

Usage::

    from event_sourcing.fitness import check_projection_purity, Violation

    def test_projections_are_pure():
        for path in find_projection_files():
            violations = check_projection_purity(path)
            assert not violations, f"{path}: {violations}"
"""

from event_sourcing.fitness.projection_purity import (
    PROJECTION_ALLOWED_PREFIXES,
    check_projection_purity,
)
from event_sourcing.fitness.violations import Violation

__all__ = [
    "PROJECTION_ALLOWED_PREFIXES",
    "Violation",
    "check_projection_purity",
]
