"""Tests for DispatchContext and SIDE_EFFECTS_ALLOWED marker."""

import pytest

from event_sourcing.core.checkpoint import (
    CheckpointedProjection,
    DispatchContext,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.event import DomainEvent, EventEnvelope
from event_sourcing.core.process_manager import ProcessManager

# ============================================================================
# DispatchContext Tests
# ============================================================================


class TestDispatchContext:
    """Tests for DispatchContext frozen dataclass."""

    def test_create_catching_up(self) -> None:
        """Should create context in catch-up mode."""
        ctx = DispatchContext(
            is_catching_up=True,
            global_nonce=5,
            live_boundary_nonce=100,
        )
        assert ctx.is_catching_up is True
        assert ctx.global_nonce == 5
        assert ctx.live_boundary_nonce == 100

    def test_create_live(self) -> None:
        """Should create context in live mode."""
        ctx = DispatchContext(
            is_catching_up=False,
            global_nonce=101,
            live_boundary_nonce=100,
        )
        assert ctx.is_catching_up is False
        assert ctx.global_nonce == 101
        assert ctx.live_boundary_nonce == 100

    def test_is_live_property(self) -> None:
        """is_live should be the inverse of is_catching_up."""
        catching_up = DispatchContext(
            is_catching_up=True, global_nonce=1, live_boundary_nonce=100,
        )
        live = DispatchContext(
            is_catching_up=False, global_nonce=101, live_boundary_nonce=100,
        )
        assert catching_up.is_live is False
        assert live.is_live is True

    def test_frozen(self) -> None:
        """DispatchContext should be immutable."""
        ctx = DispatchContext(
            is_catching_up=True, global_nonce=1, live_boundary_nonce=100,
        )
        with pytest.raises(AttributeError):
            ctx.is_catching_up = False  # type: ignore[misc]

    def test_boundary_semantics(self) -> None:
        """Events at boundary nonce are historical, above are live.

        The transition uses > (strictly greater than). Events at the boundary
        were already in the store when we subscribed.
        """
        at_boundary = DispatchContext(
            is_catching_up=True, global_nonce=100, live_boundary_nonce=100,
        )
        # At the boundary: still catching up
        assert at_boundary.is_catching_up is True

        past_boundary = DispatchContext(
            is_catching_up=False, global_nonce=101, live_boundary_nonce=100,
        )
        # Past the boundary: live
        assert past_boundary.is_live is True


# ============================================================================
# SIDE_EFFECTS_ALLOWED Tests
# ============================================================================


class TestSideEffectsAllowed:
    """Tests for SIDE_EFFECTS_ALLOWED ClassVar marker."""

    def test_projection_disallows_side_effects(self) -> None:
        """CheckpointedProjection should have SIDE_EFFECTS_ALLOWED=False."""
        assert CheckpointedProjection.SIDE_EFFECTS_ALLOWED is False

    def test_process_manager_allows_side_effects(self) -> None:
        """ProcessManager should have SIDE_EFFECTS_ALLOWED=True."""
        assert ProcessManager.SIDE_EFFECTS_ALLOWED is True

    def test_projection_subclass_inherits_false(self) -> None:
        """A projection subclass should inherit SIDE_EFFECTS_ALLOWED=False."""

        class MyProjection(CheckpointedProjection):
            def get_name(self) -> str:
                return "my_projection"

            def get_version(self) -> int:
                return 1

            def get_subscribed_event_types(self) -> set[str]:
                return {"TestEvent"}

            async def handle_event(
                self, envelope: EventEnvelope[DomainEvent],
                checkpoint_store: ProjectionCheckpointStore,
                context: DispatchContext | None = None,
            ) -> ProjectionResult:
                return ProjectionResult.SUCCESS

        assert MyProjection.SIDE_EFFECTS_ALLOWED is False

    def test_process_manager_subclass_inherits_true(self) -> None:
        """A ProcessManager subclass should inherit SIDE_EFFECTS_ALLOWED=True."""

        class MyManager(ProcessManager):
            def get_name(self) -> str:
                return "my_manager"

            def get_version(self) -> int:
                return 1

            def get_subscribed_event_types(self) -> set[str]:
                return {"TestEvent"}

            async def handle_event(
                self, envelope: EventEnvelope[DomainEvent],
                checkpoint_store: ProjectionCheckpointStore,
                context: DispatchContext | None = None,
            ) -> ProjectionResult:
                return ProjectionResult.SUCCESS

            async def process_pending(self) -> int:
                return 0

            def get_idempotency_key(self, todo_item: dict[str, str | int | float | bool | None]) -> str:
                return str(todo_item.get("id", ""))

        assert MyManager.SIDE_EFFECTS_ALLOWED is True
