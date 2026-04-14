"""Tests for the built-in fitness module.

Tests cover:
1. Projection purity check (whitelist-based import analysis)
2. ProcessManager structure check
3. Violation dataclass
"""

from __future__ import annotations

from textwrap import dedent
from typing import TYPE_CHECKING

import pytest

from event_sourcing.core.checkpoint import (
    DispatchContext,
    ProjectionCheckpointStore,
    ProjectionResult,
)
from event_sourcing.core.event import DomainEvent, EventEnvelope
from event_sourcing.core.process_manager import ProcessManager
from event_sourcing.fitness.process_manager_check import check_process_manager
from event_sourcing.fitness.projection_purity import (
    PROJECTION_ALLOWED_PREFIXES,
    check_projection_purity,
)
from event_sourcing.fitness.violations import Violation

if TYPE_CHECKING:
    from pathlib import Path

# ============================================================================
# Violation Tests
# ============================================================================


class TestViolation:
    """Tests for the Violation dataclass."""

    def test_create_violation(self) -> None:
        """Should create a violation with all fields."""
        v = Violation(
            file_path="projection.py",
            line_number=10,
            rule="projection-purity",
            message="Bad import",
        )
        assert v.file_path == "projection.py"
        assert v.line_number == 10
        assert v.rule == "projection-purity"
        assert v.message == "Bad import"

    def test_violation_is_frozen(self) -> None:
        """Violation should be immutable."""
        v = Violation(
            file_path="test.py", line_number=1, rule="test", message="test",
        )
        with pytest.raises(AttributeError):
            v.message = "changed"  # type: ignore[misc]


# ============================================================================
# Projection Purity Tests
# ============================================================================


class TestProjectionPurity:
    """Tests for whitelist-based projection purity check."""

    def test_pure_projection_passes(self, tmp_path: Path) -> None:
        """A projection with only allowed imports should have no violations."""
        source = dedent("""\
            from __future__ import annotations
            import logging
            from typing import Optional
            from datetime import datetime
            from event_sourcing.core.checkpoint import ProjectionResult

            class MyProjection:
                pass
        """)
        f = tmp_path / "pure_projection.py"
        f.write_text(source)

        violations = check_projection_purity(f)
        assert violations == []

    def test_httpx_import_flagged(self, tmp_path: Path) -> None:
        """Importing httpx should be flagged as a purity violation."""
        source = dedent("""\
            import httpx

            class BadProjection:
                pass
        """)
        f = tmp_path / "bad_projection.py"
        f.write_text(source)

        violations = check_projection_purity(f)
        assert len(violations) == 1
        assert "httpx" in violations[0].message
        assert violations[0].rule == "projection-purity"

    def test_from_import_flagged(self, tmp_path: Path) -> None:
        """from X import Y where X is not whitelisted should be flagged."""
        source = dedent("""\
            from requests import get

            class BadProjection:
                pass
        """)
        f = tmp_path / "bad_projection.py"
        f.write_text(source)

        violations = check_projection_purity(f)
        assert len(violations) == 1
        assert "requests" in violations[0].message

    def test_type_checking_import_allowed(self, tmp_path: Path) -> None:
        """Imports inside TYPE_CHECKING blocks should always be allowed."""
        source = dedent("""\
            from __future__ import annotations
            from typing import TYPE_CHECKING

            if TYPE_CHECKING:
                import httpx
                from docker import DockerClient

            class PureProjection:
                pass
        """)
        f = tmp_path / "pure_projection.py"
        f.write_text(source)

        violations = check_projection_purity(f)
        assert violations == []

    def test_multiple_violations(self, tmp_path: Path) -> None:
        """Should flag all non-whitelisted imports."""
        source = dedent("""\
            import httpx
            import subprocess
            from boto3 import client

            class ReallyBadProjection:
                pass
        """)
        f = tmp_path / "multi_bad.py"
        f.write_text(source)

        violations = check_projection_purity(f)
        assert len(violations) == 3

    def test_project_specific_allowed_prefixes(self, tmp_path: Path) -> None:
        """Project-specific prefixes should be allowed when provided."""
        source = dedent("""\
            from syn_domain.contexts.github.events import TriggerFiredEvent
            from syn_shared.settings import Settings

            class SynProjection:
                pass
        """)
        f = tmp_path / "syn_projection.py"
        f.write_text(source)

        # Without project-specific: violations
        violations = check_projection_purity(f)
        assert len(violations) == 2

        # With project-specific: no violations
        violations = check_projection_purity(
            f, allowed_prefixes={"syn_domain", "syn_shared"},
        )
        assert violations == []

    def test_event_sourcing_submodules_allowed(self, tmp_path: Path) -> None:
        """All event_sourcing.* imports should be allowed."""
        source = dedent("""\
            from event_sourcing.core.checkpoint import ProjectionResult
            from event_sourcing.core.event import DomainEvent
            from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore
        """)
        f = tmp_path / "es_imports.py"
        f.write_text(source)

        violations = check_projection_purity(f)
        assert violations == []

    def test_syntax_error_returns_violation(self, tmp_path: Path) -> None:
        """Files with syntax errors should produce a single violation."""
        f = tmp_path / "broken.py"
        f.write_text("def broken(:\n")

        violations = check_projection_purity(f)
        assert len(violations) == 1
        assert "SyntaxError" in violations[0].message

    def test_default_prefixes_include_stdlib(self) -> None:
        """Default allowed prefixes should include common stdlib modules."""
        expected = {"typing", "logging", "datetime", "uuid", "enum", "dataclasses", "abc"}
        for module in expected:
            assert module in PROJECTION_ALLOWED_PREFIXES, f"{module} not in default allowed"

    def test_default_prefixes_include_esp(self) -> None:
        """Default allowed prefixes should include event_sourcing."""
        assert "event_sourcing" in PROJECTION_ALLOWED_PREFIXES


# ============================================================================
# ProcessManager Structure Check Tests
# ============================================================================


class TestProcessManagerCheck:
    """Tests for ProcessManager structure validation."""

    def test_valid_process_manager(self) -> None:
        """A complete ProcessManager should have no violations."""

        class ValidPM(ProcessManager):
            def get_name(self) -> str:
                return "valid"

            def get_version(self) -> int:
                return 1

            def get_subscribed_event_types(self) -> set[str]:
                return set()

            async def handle_event(
                self, envelope: EventEnvelope[DomainEvent],
                checkpoint_store: ProjectionCheckpointStore,
                context: DispatchContext | None = None,
            ) -> ProjectionResult:
                return ProjectionResult.SUCCESS

            async def process_pending(self) -> int:
                return 0

            def get_idempotency_key(self, todo_item: dict[str, object]) -> str:
                return ""

        violations = check_process_manager(ValidPM)
        assert violations == []

    def test_side_effects_overridden_to_false(self) -> None:
        """Should flag ProcessManager with SIDE_EFFECTS_ALLOWED=False."""

        class BadPM(ProcessManager):
            SIDE_EFFECTS_ALLOWED = False  # type: ignore[assignment]

            def get_name(self) -> str:
                return "bad"

            def get_version(self) -> int:
                return 1

            def get_subscribed_event_types(self) -> set[str]:
                return set()

            async def handle_event(
                self, envelope: EventEnvelope[DomainEvent],
                checkpoint_store: ProjectionCheckpointStore,
                context: DispatchContext | None = None,
            ) -> ProjectionResult:
                return ProjectionResult.SUCCESS

            async def process_pending(self) -> int:
                return 0

            def get_idempotency_key(self, todo_item: dict[str, object]) -> str:
                return ""

        violations = check_process_manager(BadPM)
        assert len(violations) == 1
        assert "SIDE_EFFECTS_ALLOWED" in violations[0].message


# ============================================================================
# Test Kit Tests
# ============================================================================


class TestProcessManagerScenario:
    """Tests for the ProcessManagerScenario test utility."""

    @pytest.mark.asyncio
    async def test_given_events_does_not_call_process_pending(self) -> None:
        """given_events() should dispatch in catch-up mode - no process_pending."""
        from event_sourcing.testing import ProcessManagerScenario

        class TestPM(ProcessManager):
            def __init__(self) -> None:
                self.events_received: list[str] = []

            def get_name(self) -> str:
                return "test_pm"

            def get_version(self) -> int:
                return 1

            def get_subscribed_event_types(self) -> set[str]:
                return {"TestEvent"}

            async def handle_event(
                self, envelope: EventEnvelope[DomainEvent],
                checkpoint_store: ProjectionCheckpointStore,
                context: DispatchContext | None = None,
            ) -> ProjectionResult:
                self.events_received.append(envelope.metadata.event_type or "")
                return ProjectionResult.SUCCESS

            async def process_pending(self) -> int:
                return 0

            def get_idempotency_key(self, todo_item: dict[str, object]) -> str:
                return ""

        from event_sourcing.core.event import EventMetadata

        pm = TestPM()
        scenario = ProcessManagerScenario(pm)

        envelopes = []
        for i in range(3):

            class SimpleEvent(DomainEvent):
                event_type = "TestEvent"

            envelopes.append(EventEnvelope(
                event=SimpleEvent(),
                metadata=EventMetadata(
                    aggregate_nonce=1,
                    aggregate_id=f"agg-{i}",
                    aggregate_type="Test",
                    global_nonce=i + 1,
                    event_type="TestEvent",
                ),
            ))

        await scenario.given_events(envelopes)

        assert len(pm.events_received) == 3
        assert scenario.process_pending_call_count == 0

    @pytest.mark.asyncio
    async def test_when_live_event_calls_process_pending(self) -> None:
        """when_live_event() should call process_pending() after handle_event."""
        from event_sourcing.testing import ProcessManagerScenario

        class TestPM(ProcessManager):
            def __init__(self) -> None:
                self.pending_called = False

            def get_name(self) -> str:
                return "test_pm"

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
                self.pending_called = True
                return 1

            def get_idempotency_key(self, todo_item: dict[str, object]) -> str:
                return ""

        from event_sourcing.core.event import EventMetadata

        class SimpleEvent(DomainEvent):
            event_type = "TestEvent"

        pm = TestPM()
        scenario = ProcessManagerScenario(pm)

        envelope = EventEnvelope(
            event=SimpleEvent(),
            metadata=EventMetadata(
                aggregate_nonce=1,
                aggregate_id="agg-1",
                aggregate_type="Test",
                global_nonce=10,
                event_type="TestEvent",
            ),
        )

        result = await scenario.when_live_event(envelope)
        assert result == ProjectionResult.SUCCESS
        assert pm.pending_called is True


class TestIdempotencyVerifier:
    """Tests for the IdempotencyVerifier test utility."""

    @pytest.mark.asyncio
    async def test_idempotent_process_manager(self) -> None:
        """Idempotent process_pending should return is_idempotent=True."""
        from event_sourcing.testing import IdempotencyVerifier

        class IdempotentPM(ProcessManager):
            def __init__(self) -> None:
                self._first_call = True

            def get_name(self) -> str:
                return "idem"

            def get_version(self) -> int:
                return 1

            def get_subscribed_event_types(self) -> set[str]:
                return set()

            async def handle_event(
                self, envelope: EventEnvelope[DomainEvent],
                checkpoint_store: ProjectionCheckpointStore,
                context: DispatchContext | None = None,
            ) -> ProjectionResult:
                return ProjectionResult.SUCCESS

            async def process_pending(self) -> int:
                if self._first_call:
                    self._first_call = False
                    return 3
                return 0  # Idempotent: second call processes nothing

            def get_idempotency_key(self, todo_item: dict[str, object]) -> str:
                return ""

        pm = IdempotentPM()
        verifier = IdempotencyVerifier(pm)
        result = await verifier.verify(expected_first_pass=3)

        assert result.first_pass_count == 3
        assert result.second_pass_count == 0
        assert result.is_idempotent is True

    @pytest.mark.asyncio
    async def test_non_idempotent_detected(self) -> None:
        """Non-idempotent process_pending should return is_idempotent=False."""
        from event_sourcing.testing import IdempotencyVerifier

        class NonIdempotentPM(ProcessManager):
            def get_name(self) -> str:
                return "non_idem"

            def get_version(self) -> int:
                return 1

            def get_subscribed_event_types(self) -> set[str]:
                return set()

            async def handle_event(
                self, envelope: EventEnvelope[DomainEvent],
                checkpoint_store: ProjectionCheckpointStore,
                context: DispatchContext | None = None,
            ) -> ProjectionResult:
                return ProjectionResult.SUCCESS

            async def process_pending(self) -> int:
                return 2  # Always processes 2, not idempotent

            def get_idempotency_key(self, todo_item: dict[str, object]) -> str:
                return ""

        pm = NonIdempotentPM()
        verifier = IdempotencyVerifier(pm)
        result = await verifier.verify()

        assert result.second_pass_count == 2
        assert result.is_idempotent is False
