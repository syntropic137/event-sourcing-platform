"""
Validate HistoricalPoller subclasses preserve the cold-start fence.

The core invariant: ``poll()`` is a concrete template method that
enforces cold-start safety. Subclasses must NOT override it -- doing
so would bypass the timestamp fence and re-introduce cold-start
event floods.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from event_sourcing.fitness.violations import Violation

if TYPE_CHECKING:
    from event_sourcing.core.historical_poller import HistoricalPoller


def check_historical_poller_structure(cls: type[HistoricalPoller]) -> list[Violation]:
    """Validate a HistoricalPoller subclass preserves the cold-start fence.

    Checks:
    1. ``poll()`` is not overridden (template method must stay intact).
    2. ``fetch()`` and ``process()`` are concretely implemented.

    Args:
        cls: The HistoricalPoller subclass to validate.

    Returns:
        List of violations. Empty means the class is valid.
    """
    violations: list[Violation] = []
    source_file = _get_source_file(cls)

    # Check poll() is NOT overridden -- it's the cold-start fence
    if "poll" in cls.__dict__:
        violations.append(
            Violation(
                file_path=source_file,
                line_number=0,
                rule="historical-poller-fence",
                message=(
                    f"{cls.__name__} overrides poll(). "
                    f"poll() is a template method that enforces the cold-start fence. "
                    f"Override fetch() and process() instead."
                ),
            )
        )

    # Check abstract methods are concretely implemented
    for method_name in ("fetch", "process"):
        method = getattr(cls, method_name, None)
        if method is None:
            violations.append(
                Violation(
                    file_path=source_file,
                    line_number=0,
                    rule="historical-poller-fence",
                    message=f"{cls.__name__} is missing required method '{method_name}()'",
                )
            )
        elif getattr(method, "__isabstractmethod__", False):
            violations.append(
                Violation(
                    file_path=source_file,
                    line_number=0,
                    rule="historical-poller-fence",
                    message=(
                        f"{cls.__name__}.{method_name}() is still abstract. "
                        f"Provide a concrete implementation."
                    ),
                )
            )

    # Check _prime and _persist_cursor are NOT overridden
    for internal_method in ("_prime", "_persist_cursor"):
        if internal_method in cls.__dict__:
            violations.append(
                Violation(
                    file_path=source_file,
                    line_number=0,
                    rule="historical-poller-fence",
                    message=(
                        f"{cls.__name__} overrides {internal_method}(). "
                        f"Internal cursor management must not be modified."
                    ),
                )
            )

    return violations


def _get_source_file(cls: type[HistoricalPoller]) -> str:
    """Best-effort source file path for a class."""
    import inspect

    try:
        return inspect.getfile(cls)
    except (TypeError, OSError):
        return f"<unknown: {cls.__qualname__}>"
