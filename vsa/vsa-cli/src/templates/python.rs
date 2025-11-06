//! Python templates for VSA code generation

/// Command template for Python
pub const COMMAND_TEMPLATE: &str = r#""""Command to {{feature_name}}"""

from pydantic import BaseModel


class {{command_name}}(BaseModel):
    """Command to {{feature_name}}"""

{{#each fields}}    {{name}}: {{field_type}}
{{/each}}
"#;

/// Event template for Python
pub const EVENT_TEMPLATE: &str = r#""""Event representing {{feature_name}} completion"""

{{#if framework}}from event_sourcing import DomainEvent
{{else}}from pydantic import BaseModel
{{/if}}

class {{event_name}}({{#if framework}}DomainEvent{{else}}BaseModel{{/if}}):
    """Event representing {{feature_name}} completion"""

    event_type: str = "{{event_name}}"
{{#each fields}}    {{name}}: {{field_type}}
{{/each}}
"#;

/// Handler template for Python
pub const HANDLER_TEMPLATE: &str = r#""""Handler for {{command_name}}"""

from .{{command_name}} import {{command_name}}
from .{{event_name}} import {{event_name}}
{{#if aggregate_name}}from .{{aggregate_name}} import {{aggregate_name}}
{{/if}}{{#if framework}}from event_sourcing import Repository
{{/if}}

class {{handler_name}}:
    """Handler for {{command_name}}

    This handler processes the command, applies business logic,
    creates events, and persists them to the event store.
    """

    def __init__(self, {{#if framework}}repository: Repository{{else}}event_store{{/if}}):
        {{#if framework}}self.repository = repository{{else}}self.event_store = event_store{{/if}}

    async def handle(self, command: {{command_name}}) -> None:
        """Process the command and emit events"""
        # TODO: Add validation logic

{{#if aggregate_name}}        # Load or create aggregate
        aggregate = {{aggregate_name}}()
        aggregate.set_id(command.id)

        # Apply command to aggregate
        aggregate.handle_{{operation_name}}(command)

        # Save aggregate
        await self.repository.save(aggregate)
{{else}}        # Create event
        event = {{event_name}}(
{{#each fields}}            {{name}}=command.{{name}},
{{/each}}        )

        # TODO: Persist to event store
        # await self.event_store.append(stream_name, [event])
{{/if}}
"#;

/// Test template for Python
pub const TEST_TEMPLATE: &str = r#""""Tests for {{test_name}} feature"""

import pytest

from .{{command_name}} import {{command_name}}
from .{{event_name}} import {{event_name}}
from .{{handler_name}} import {{handler_name}}


class Test{{test_name}}:
    """Tests for {{test_name}} feature"""

    def test_create_command(self):
        """Test command creation"""
        command = {{command_name}}(
{{#each fields}}            {{name}}="test_{{name}}",
{{/each}}        )
{{#each fields}}        assert command.{{name}} == "test_{{name}}"
{{/each}}

    def test_create_event(self):
        """Test event creation"""
        event = {{event_name}}(
{{#each fields}}            {{name}}="test_{{name}}",
{{/each}}        )
{{#each fields}}        assert event.{{name}} == "test_{{name}}"
{{/each}}

    @pytest.mark.asyncio
    async def test_handler_execution(self):
        """Test handler execution"""
        # TODO: Implement handler test with mock repository/event store
        pass
"#;

/// Aggregate template for Python
pub const AGGREGATE_TEMPLATE: &str = r#""""Aggregate for {{feature_name}}"""

{{#if framework}}from event_sourcing import AggregateRoot, event_sourcing_handler
{{else}}from typing import Any
{{/if}}from .{{event_name}} import {{event_name}}


class {{aggregate_name}}({{#if framework}}AggregateRoot{{else}}object{{/if}}):
    """Aggregate for {{feature_name}}

    AggregateRoot automatically routes events to their corresponding
    @event_sourcing_handler methods based on event type.
    """

    def __init__(self):
{{#if framework}}        super().__init__()
{{/if}}{{#each fields}}        self.{{name}}: {{field_type}}{{#unless is_required}} | None{{/unless}} = {{#if is_required}}""{{else}}None{{/if}}
{{/each}}

{{#if framework}}    @event_sourcing_handler("{{event_name}}")
{{/if}}    def on_{{operation_name}}(self, event: {{event_name}}) -> None:
        """Apply {{event_name}} to aggregate state"""
{{#each fields}}        self.{{name}} = event.{{name}}
{{/each}}

    def handle_{{operation_name}}(self, command: Any) -> None:
        """Handle command and raise event"""
        event = {{event_name}}(
{{#each fields}}            {{name}}=command.{{name}},
{{/each}}        )
{{#if framework}}        self._raise_event(event)
{{else}}        # TODO: Raise event
{{/if}}
"#;
