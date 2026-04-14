"""Violation types for architectural fitness checks."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Violation:
    """A single architectural fitness violation.

    Attributes:
        file_path: Path to the file containing the violation.
        line_number: Line number of the violating code (1-indexed).
        rule: Short rule identifier (e.g., "projection-purity").
        message: Human-readable description of the violation.
        severity: "error" for must-fix, "warning" for advisory.
    """

    file_path: str
    line_number: int
    rule: str
    message: str
    severity: str = "error"

    def __str__(self) -> str:
        return f"{self.file_path}:{self.line_number} [{self.rule}] {self.message}"
