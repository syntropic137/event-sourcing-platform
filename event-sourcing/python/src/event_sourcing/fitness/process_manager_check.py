"""
Validate ProcessManager subclasses implement the required interface.

Checks that classes inheriting from ``ProcessManager`` provide all three
required methods and have ``SIDE_EFFECTS_ALLOWED = True``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from event_sourcing.fitness.violations import Violation

if TYPE_CHECKING:
    from event_sourcing.core.process_manager import ProcessManager


def check_process_manager(cls: type[ProcessManager]) -> list[Violation]:
    """Validate a ProcessManager subclass implements the full interface.

    Args:
        cls: The ProcessManager subclass to validate.

    Returns:
        List of violations. Empty means the class is valid.
    """
    violations: list[Violation] = []
    source_file = _get_source_file(cls)

    # Check SIDE_EFFECTS_ALLOWED is True
    if not getattr(cls, "SIDE_EFFECTS_ALLOWED", False):
        violations.append(
            Violation(
                file_path=source_file,
                line_number=0,
                rule="process-manager-structure",
                message=(
                    f"{cls.__name__} has SIDE_EFFECTS_ALLOWED=False. "
                    f"ProcessManager subclasses must allow side effects."
                ),
            )
        )

    # Check required methods are not still abstract
    for method_name in ("process_pending", "get_idempotency_key"):
        method = getattr(cls, method_name, None)
        if method is None:
            violations.append(
                Violation(
                    file_path=source_file,
                    line_number=0,
                    rule="process-manager-structure",
                    message=f"{cls.__name__} is missing required method '{method_name}()'",
                )
            )
        elif getattr(method, "__isabstractmethod__", False):
            violations.append(
                Violation(
                    file_path=source_file,
                    line_number=0,
                    rule="process-manager-structure",
                    message=(
                        f"{cls.__name__}.{method_name}() is still abstract. "
                        f"Provide a concrete implementation."
                    ),
                )
            )

    return violations


def _get_source_file(cls: type[object]) -> str:  # OBJRATCHET: accepts any class type
    """Best-effort source file path for a class."""
    import inspect

    try:
        return inspect.getfile(cls)
    except (TypeError, OSError):
        return f"<unknown: {cls.__qualname__}>"
