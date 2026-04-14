"""
Replay safety check for ProcessManager implementations.

Verifies that ``process_pending()`` is never invoked during catch-up
replay. This is a runtime check that uses a spy wrapper around the
coordinator's dispatch path.

Usage::

    from event_sourcing.fitness.replay_safety import ReplaySafetyChecker

    checker = ReplaySafetyChecker(coordinator)
    violations = await checker.verify_replay_safety(events)
"""

from __future__ import annotations

import sys
from typing import TYPE_CHECKING
from unittest.mock import AsyncMock, patch

from event_sourcing.fitness.violations import Violation

if TYPE_CHECKING:
    from event_sourcing.core.event import DomainEvent, EventEnvelope
    from event_sourcing.subscriptions.coordinator import SubscriptionCoordinator


class ReplaySafetyChecker:
    """Verify that ProcessManagers do not run process_pending() during replay.

    Wraps the coordinator's dispatch path and monitors all ProcessManager
    instances for ``process_pending()`` calls while ``is_catching_up``
    is True.
    """

    def __init__(self, coordinator: SubscriptionCoordinator) -> None:
        self._coordinator = coordinator

    async def verify_replay_safety(
        self,
        events: list[EventEnvelope[DomainEvent]],
    ) -> list[Violation]:
        """Dispatch events in catch-up mode and verify zero process_pending() calls.

        Forces the coordinator into catch-up mode, dispatches all provided
        events, and checks that no ProcessManager's ``process_pending()``
        was called.

        Args:
            events: Events to replay through the coordinator.

        Returns:
            List of violations (one per ProcessManager that was called).
        """
        from event_sourcing.core.process_manager import ProcessManager

        violations: list[Violation] = []
        spies: dict[str, AsyncMock] = {}

        # Identify all ProcessManager instances to spy on
        for name, projection in self._coordinator.projections.items():
            if isinstance(projection, ProcessManager):
                spy = AsyncMock(return_value=0)
                spies[name] = spy

        # Force catch-up mode and prevent the catch-up -> live transition.
        # Setting is_catching_up = True alone is insufficient because
        # dispatch_event() flips it to live when global_nonce exceeds
        # live_boundary_nonce. Pin the boundary to sys.maxsize so the
        # transition never fires during the check.
        original_catching_up = self._coordinator.is_catching_up
        original_boundary = self._coordinator.live_boundary_nonce
        self._coordinator.is_catching_up = True
        self._coordinator.live_boundary_nonce = sys.maxsize

        try:
            for name, spy in spies.items():
                projection = self._coordinator.projections[name]
                with patch.object(projection, "process_pending", spy):
                    for event in events:
                        await self._coordinator.dispatch_event(event)

            # Check: no process_pending() calls during catch-up
            for name, spy in spies.items():
                if spy.call_count > 0:
                    violations.append(
                        Violation(
                            file_path=f"<runtime:{name}>",
                            line_number=0,
                            rule="replay-safety",
                            message=(
                                f"ProcessManager '{name}' had process_pending() "
                                f"called {spy.call_count} time(s) during catch-up replay"
                            ),
                        )
                    )
        finally:
            self._coordinator.is_catching_up = original_catching_up
            self._coordinator.live_boundary_nonce = original_boundary

        return violations
