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

// =============================================================================
// QUERY SLICE TEMPLATES (CQRS Read Side)
// =============================================================================

/// Query template for Python (DTO for query parameters)
pub const QUERY_TEMPLATE: &str = r#""""Query to {{feature_name}}"""

from dataclasses import dataclass


@dataclass(frozen=True)
class {{query_name}}:
    """Query DTO for {{feature_name}}"""

{{#each fields}}    {{name}}: {{field_type}}
{{/each}}
"#;

/// Projection template for Python (builds read model from events)
pub const PROJECTION_TEMPLATE: &str = r#""""Projection for {{feature_name}}"""

from dataclasses import dataclass, field
from typing import Optional

from event_sourcing import Projection, handles
{{#each subscribed_events}}from domain.events import {{this}}
{{/each}}


@dataclass
class {{read_model_name}}:
    """Read model for {{feature_name}}"""

{{#each fields}}    {{name}}: {{field_type}}
{{/each}}


class {{projection_name}}(Projection[{{read_model_name}}]):
    """Projection that builds {{read_model_name}} from events.

    This projection processes domain events to build an optimized
    read model for the specific query needs.
    """

    def __init__(self) -> None:
        self._items: dict[str, {{read_model_name}}] = {}

{{#each subscribed_events}}
    @handles({{this}})
    async def on_{{strip_event_suffix this}}(self, event: {{this}}) -> None:
        """Handle {{this}} event"""
        # TODO: Update read model based on event data
        # item = self._items.get(event.aggregate_id) or self._create_default()
        # self._items[event.aggregate_id] = updated_item
        pass
{{/each}}

    def get_all(self) -> list[{{read_model_name}}]:
        """Get all items in the read model"""
        return list(self._items.values())

    def get_by_id(self, id: str) -> Optional[{{read_model_name}}]:
        """Get a specific item by ID"""
        return self._items.get(id)

    def _create_default(self) -> {{read_model_name}}:
        """Create a default read model item"""
        # TODO: Return default read model structure
        return {{read_model_name}}(
{{#each fields}}            {{name}}="",
{{/each}}        )
"#;

/// Query handler template for Python
pub const QUERY_HANDLER_TEMPLATE: &str = r#""""Handler for {{query_name}}"""

from typing import {{#if is_list}}list{{else}}Optional{{/if}}

from .{{query_name}} import {{query_name}}
from .{{projection_name}} import {{projection_name}}, {{read_model_name}}


class {{query_handler_name}}:
    """Handler for {{query_name}}.

    This handler executes the query against the projection's read model.
    Handlers are thin - they simply retrieve data from the projection.
    """

    def __init__(self, projection: {{projection_name}}) -> None:
        self._projection = projection

    async def handle(self, query: {{query_name}}) -> {{#if is_list}}list[{{read_model_name}}]{{else}}{{read_model_name}}{{/if}}:
        """Execute the query and return results"""
{{#if is_list}}
        return self._projection.get_all()
{{else}}
        result = self._projection.get_by_id(query.id)
        if result is None:
            raise ValueError(f"{{read_model_name}} not found")
        return result
{{/if}}
"#;

/// Query controller template for Python (thin adapter for HTTP/REST)
pub const QUERY_CONTROLLER_TEMPLATE: &str = r#""""Controller for {{feature_name}}"""

from fastapi import APIRouter, Depends, HTTPException

from infrastructure import QueryBus
from .{{query_name}} import {{query_name}}

router = APIRouter()


class {{controller_name}}:
    """Controller for {{feature_name}}.

    This is a thin adapter that translates HTTP requests to query bus calls.
    Controllers should NOT contain business logic - only request/response mapping.
    """

    def __init__(self, query_bus: QueryBus) -> None:
        self._query_bus = query_bus

    async def handle(
        self,
{{#each fields}}        {{name}}: {{field_type}},
{{/each}}    ):
        """Handle GET request"""
        query = {{query_name}}(
{{#each fields}}            {{name}}={{name}},
{{/each}}        )

        try:
            result = await self._query_bus.execute(query)
            return result
        except ValueError as e:
            raise HTTPException(status_code=404, detail=str(e))


# FastAPI route
@router.get("/{{feature_name}}")
async def {{feature_name}}_endpoint(
{{#each fields}}    {{name}}: {{field_type}},
{{/each}}    controller: {{controller_name}} = Depends(),
):
    """{{feature_name}} endpoint"""
    return await controller.handle({{#each fields}}{{name}},{{/each}})
"#;

/// Query test template for Python
pub const QUERY_TEST_TEMPLATE: &str = r#""""Tests for {{feature_name}} query slice"""

import pytest

from .{{query_name}} import {{query_name}}
from .{{projection_name}} import {{projection_name}}, {{read_model_name}}
from .{{query_handler_name}} import {{query_handler_name}}


class Test{{operation_name}}:
    """Tests for {{feature_name}} query slice"""

    @pytest.fixture
    def projection(self) -> {{projection_name}}:
        """Create projection fixture"""
        return {{projection_name}}()

    @pytest.fixture
    def handler(self, projection: {{projection_name}}) -> {{query_handler_name}}:
        """Create handler fixture"""
        return {{query_handler_name}}(projection)

    def test_create_query(self):
        """Test query creation"""
        query = {{query_name}}(
{{#each fields}}            {{name}}="test_{{name}}",
{{/each}}        )
{{#each fields}}        assert query.{{name}} == "test_{{name}}"
{{/each}}

    def test_projection_starts_empty(self, projection: {{projection_name}}):
        """Test projection starts with empty read model"""
        items = projection.get_all()
        assert len(items) == 0

    @pytest.mark.asyncio
    async def test_handler_execution(
        self,
        handler: {{query_handler_name}},
    ):
        """Test handler execution"""
        query = {{query_name}}(
{{#each fields}}            {{name}}="test_{{name}}",
{{/each}}        )

        # TODO: Setup projection with test data first
        # result = await handler.handle(query)
        # assert result is not None
        pass
"#;

/// Slice manifest template for Python (slice.yaml)
pub const SLICE_MANIFEST_TEMPLATE: &str = r#"name: {{feature_name}}
type: {{slice_type}}
{{#if projection_name}}projection: {{projection_name}}
{{/if}}{{#if subscribed_events}}subscribes_to:
{{#each subscribed_events}}  - {{this}}
{{/each}}{{/if}}{{#if read_model_name}}returns: {{read_model_name}}
{{/if}}description: |
  {{#if is_list}}Lists {{feature_name}} from the read model.{{else}}Gets {{feature_name}} details from the read model.{{/if}}
"#;
