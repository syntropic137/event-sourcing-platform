"""
Test utilities for ProcessManager implementations.

Provides tools to test the two sides of a ProcessManager independently:

- ``ProcessManagerScenario``: Test the projection side (handle_event)
  in isolation, verifying that to-do records are written correctly
  and that process_pending() is never called during replay.

- ``IdempotencyVerifier``: Test that process_pending() is idempotent
  by calling it multiple times and verifying effects happen once.

Example::

    from event_sourcing.testing import ProcessManagerScenario

    async def test_dispatch_manager_projection_side():
        manager = WorkflowDispatchManager(store=mock_store)
        scenario = ProcessManagerScenario(manager)

        await scenario.given_events([
            TriggerFiredEvent(trigger_id="t1", execution_id="e1"),
        ])

        assert scenario.process_pending_call_count == 0
        assert await mock_store.query(status="pending") == [...]
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from event_sourcing.core.checkpoint import (
    DispatchContext,
    ProjectionResult,
)
from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore

if TYPE_CHECKING:
    from event_sourcing.core.event import DomainEvent, EventEnvelope
    from event_sourcing.core.process_manager import ProcessManager


class ProcessManagerScenario:
    """Test the projection side of a ProcessManager in isolation.

    Dispatches events through ``handle_event()`` with
    ``is_catching_up=True`` (replay mode) and tracks whether
    ``process_pending()`` was called (it should never be during replay).
    """

    def __init__(self, process_manager: ProcessManager) -> None:
        self._pm = process_manager
        self._checkpoint_store = MemoryCheckpointStore()
        self._process_pending_call_count = 0
        self._results: list[ProjectionResult] = []

    @property
    def process_pending_call_count(self) -> int:
        """Number of times process_pending() was called. Should be 0 after replay."""
        return self._process_pending_call_count

    @property
    def results(self) -> list[ProjectionResult]:
        """Results from each handle_event() call."""
        return list(self._results)

    async def given_events(
        self,
        events: list[EventEnvelope[DomainEvent]],
    ) -> ProcessManagerScenario:
        """Replay events through the projection side in catch-up mode.

        All events are dispatched with ``is_catching_up=True``. The
        scenario verifies that ``process_pending()`` is not called.

        Args:
            events: Events to replay.

        Returns:
            Self for chaining.
        """
        # Wrap process_pending to track calls
        original = self._pm.process_pending

        async def _spy_process_pending() -> int:
            self._process_pending_call_count += 1
            return await original()

        self._pm.process_pending = _spy_process_pending  # type: ignore[method-assign]

        try:
            for i, envelope in enumerate(events):
                global_nonce = envelope.metadata.global_nonce or i
                context = DispatchContext(
                    is_catching_up=True,
                    global_nonce=global_nonce,
                    live_boundary_nonce=len(events),
                )
                result = await self._pm.handle_event(
                    envelope, self._checkpoint_store, context,
                )
                self._results.append(result)
        finally:
            self._pm.process_pending = original  # type: ignore[method-assign]

        return self

    async def when_live_event(
        self,
        envelope: EventEnvelope[DomainEvent],
    ) -> ProjectionResult:
        """Process a single event in live mode.

        Dispatches with ``is_catching_up=False`` and then calls
        ``process_pending()``, mimicking what the coordinator does
        for live events.

        Args:
            envelope: The live event to process.

        Returns:
            The ProjectionResult from handle_event().
        """
        global_nonce = envelope.metadata.global_nonce or 0
        context = DispatchContext(
            is_catching_up=False,
            global_nonce=global_nonce,
            live_boundary_nonce=global_nonce - 1,
        )
        result = await self._pm.handle_event(
            envelope, self._checkpoint_store, context,
        )
        if result == ProjectionResult.SUCCESS:
            await self._pm.process_pending()
        return result


class IdempotencyVerifier:
    """Verify that process_pending() is idempotent.

    Calls process_pending() multiple times and checks that the total
    number of items processed does not increase after the first call
    (all items should be marked as done after the first pass).
    """

    def __init__(self, process_manager: ProcessManager) -> None:
        self._pm = process_manager

    async def verify(self, expected_first_pass: int = 0) -> IdempotencyResult:
        """Run process_pending() twice and compare results.

        Args:
            expected_first_pass: Expected number of items on first call.

        Returns:
            IdempotencyResult with pass counts and whether idempotency holds.
        """
        first_count = await self._pm.process_pending()
        second_count = await self._pm.process_pending()

        return IdempotencyResult(
            first_pass_count=first_count,
            second_pass_count=second_count,
            is_idempotent=second_count == 0,
            expected_first_pass=expected_first_pass,
        )


class IdempotencyResult:
    """Result of an idempotency verification."""

    def __init__(
        self,
        first_pass_count: int,
        second_pass_count: int,
        is_idempotent: bool,
        expected_first_pass: int,
    ) -> None:
        self.first_pass_count = first_pass_count
        self.second_pass_count = second_pass_count
        self.is_idempotent = is_idempotent
        self.expected_first_pass = expected_first_pass

    def __repr__(self) -> str:
        status = "PASS" if self.is_idempotent else "FAIL"
        return (
            f"IdempotencyResult({status}: "
            f"first={self.first_pass_count}, second={self.second_pass_count})"
        )
