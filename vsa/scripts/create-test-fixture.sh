#!/bin/bash
# VSA Core Test Fixture Generator
# Usage: ./create-test-fixture.sh <language> <fixture-name> <valid|invalid>

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="$SCRIPT_DIR/../vsa-core/tests/fixtures"

LANGUAGE=$1
FIXTURE_NAME=$2
VALIDITY=$3

if [ -z "$LANGUAGE" ] || [ -z "$FIXTURE_NAME" ] || [ -z "$VALIDITY" ]; then
    echo "Usage: ./create-test-fixture.sh <language> <fixture-name> <valid|invalid>"
    echo ""
    echo "Arguments:"
    echo "  language:     typescript, python, or rust"
    echo "  fixture-name: Name of the fixture (e.g., 'hexagonal-vsa' or 'domain-imports')"
    echo "  validity:     valid or invalid"
    echo ""
    echo "Examples:"
    echo "  ./create-test-fixture.sh typescript hexagonal-vsa valid"
    echo "  ./create-test-fixture.sh typescript domain-imports invalid"
    exit 1
fi

if [ "$VALIDITY" != "valid" ] && [ "$VALIDITY" != "invalid" ]; then
    echo "Error: validity must be 'valid' or 'invalid'"
    exit 1
fi

FULL_FIXTURE_NAME="${VALIDITY}-${FIXTURE_NAME}"
FIXTURE_PATH="$FIXTURES_DIR/$LANGUAGE/$FULL_FIXTURE_NAME"

echo "Creating fixture: $FULL_FIXTURE_NAME"
echo "Language: $LANGUAGE"
echo "Path: $FIXTURE_PATH"
echo ""

# Check if fixture already exists
if [ -d "$FIXTURE_PATH" ]; then
    echo "Error: Fixture already exists at $FIXTURE_PATH"
    exit 1
fi

# Create fixture directory
mkdir -p "$FIXTURE_PATH"

# Generate structure based on language
case $LANGUAGE in
    typescript)
        create_typescript_fixture "$FIXTURE_PATH" "$FIXTURE_NAME" "$VALIDITY"
        ;;
    python)
        create_python_fixture "$FIXTURE_PATH" "$FIXTURE_NAME" "$VALIDITY"
        ;;
    rust)
        create_rust_fixture "$FIXTURE_PATH" "$FIXTURE_NAME" "$VALIDITY"
        ;;
    *)
        echo "Error: Unknown language '$LANGUAGE'"
        exit 1
        ;;
esac

echo ""
echo "✅ Fixture created successfully!"
echo ""
echo "Next steps:"
echo "  1. Edit the generated files in: $FIXTURE_PATH"
echo "  2. Update README.md with test scenario details"
echo "  3. Add E2E test in: vsa-core/tests/e2e/fixtures_${LANGUAGE}_test.rs"
echo ""

# ============================================================================
# TypeScript Fixture Generator
# ============================================================================

create_typescript_fixture() {
    local path=$1
    local name=$2
    local validity=$3
    
    echo "Generating TypeScript fixture structure..."
    
    # Create directories
    mkdir -p "$path/domain/commands/tasks"
    mkdir -p "$path/domain/queries"
    mkdir -p "$path/domain/events/_versioned"
    mkdir -p "$path/domain/events/_upcasters"
    mkdir -p "$path/infrastructure"
    mkdir -p "$path/slices/create-task"
    
    # vsa.yaml
    cat > "$path/vsa.yaml" << 'EOF'
version: 2
architecture: "hexagonal-event-sourced-vsa"
language: "typescript"

domain:
  path: "domain"
  aggregates:
    path: "."
    pattern: "*Aggregate.ts"
    require_suffix: true
  commands:
    path: "commands"
    pattern: "**/*Command.ts"
    require_suffix: true
    organize_by_feature: true
  events:
    path: "events"
    pattern: "**/*Event.ts"
    require_suffix: true
    versioning:
      enabled: true
      format: "simple"
      require_decorator: true
      require_upcasters: true

slices:
  path: "slices"
  command:
    must_use: "CommandBus"
    max_lines: 50
    no_business_logic: true

validation:
  architecture:
    enforce_hexagonal: true
    slices_isolated: true
  event_sourcing:
    require_event_versioning: true
EOF
    
    # package.json
    cat > "$path/package.json" << 'EOF'
{
  "name": "vsa-test-fixture",
  "version": "1.0.0",
  "private": true,
  "dependencies": {
    "typescript": "^5.0.0"
  }
}
EOF
    
    # tsconfig.json
    cat > "$path/tsconfig.json" << 'EOF'
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "strict": true,
    "esModuleInterop": true,
    "experimentalDecorators": true,
    "emitDecoratorMetadata": true
  }
}
EOF
    
    # Domain: TaskAggregate.ts
    cat > "$path/domain/TaskAggregate.ts" << 'EOF'
import { Aggregate, CommandHandler, EventSourcingHandler } from '@vsa/core';
import { CreateTaskCommand } from './commands/tasks/CreateTaskCommand';
import { TaskCreatedEvent } from './events/TaskCreatedEvent';

@Aggregate()
export class TaskAggregate {
  private id: string;
  private title: string;
  private completed: boolean = false;

  @CommandHandler
  handle(command: CreateTaskCommand): void {
    if (!command.title || command.title.length === 0) {
      throw new Error('Task title is required');
    }

    this.apply(new TaskCreatedEvent(
      command.aggregateId,
      command.title
    ));
  }

  @EventSourcingHandler
  on(event: TaskCreatedEvent): void {
    this.id = event.aggregateId;
    this.title = event.title;
  }
}
EOF
    
    # Domain: CreateTaskCommand.ts
    cat > "$path/domain/commands/tasks/CreateTaskCommand.ts" << 'EOF'
export class CreateTaskCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly title: string
  ) {}
}
EOF
    
    # Domain: TaskCreatedEvent.ts
    cat > "$path/domain/events/TaskCreatedEvent.ts" << 'EOF'
import { Event } from '@vsa/core';

@Event('TaskCreated', 'v1')
export class TaskCreatedEvent {
  constructor(
    public readonly aggregateId: string,
    public readonly title: string
  ) {}
}
EOF
    
    # Infrastructure: CommandBus.ts
    cat > "$path/infrastructure/CommandBus.ts" << 'EOF'
export class CommandBus {
  async send(command: any): Promise<void> {
    // Implementation
  }
}
EOF
    
    # Slice: CreateTaskController.ts
    cat > "$path/slices/create-task/CreateTaskController.ts" << 'EOF'
import { RestController, Post, Body } from '@vsa/adapters';
import { CommandBus } from '../../infrastructure/CommandBus';
import { CreateTaskCommand } from '../../domain/commands/tasks/CreateTaskCommand';

@RestController('/api/tasks')
export class CreateTaskController {
  constructor(private commandBus: CommandBus) {}

  @Post('/')
  async handle(@Body() request: CreateTaskRequest): Promise<void> {
    const command = new CreateTaskCommand(
      this.generateId(),
      request.title
    );
    await this.commandBus.send(command);
  }

  private generateId(): string {
    return crypto.randomUUID();
  }
}

interface CreateTaskRequest {
  title: string;
}
EOF
    
    # Slice: slice.yaml
    cat > "$path/slices/create-task/slice.yaml" << 'EOF'
name: "create-task"
type: "command"

command:
  command_type: "CreateTaskCommand"
  aggregate: "TaskAggregate"

adapters:
  rest:
    enabled: true
    routes:
      - method: "POST"
        path: "/api/tasks"
EOF
    
    # README.md
    cat > "$path/README.md" << EOF
# Test Fixture: ${FULL_FIXTURE_NAME}

**Language:** TypeScript  
**Validity:** ${VALIDITY}  
**Purpose:** [TODO: Describe what this fixture tests]

## Scenario

[TODO: Describe the test scenario]

## Expected Behavior

**Valid:** [Expected behavior if valid]
**Invalid:** [Expected errors if invalid]

## Structure

\`\`\`
${FULL_FIXTURE_NAME}/
├── vsa.yaml
├── domain/
│   ├── TaskAggregate.ts
│   ├── commands/
│   │   └── tasks/
│   │       └── CreateTaskCommand.ts
│   └── events/
│       └── TaskCreatedEvent.ts
├── infrastructure/
│   └── CommandBus.ts
└── slices/
    └── create-task/
        ├── CreateTaskController.ts
        └── slice.yaml
\`\`\`

## Validation

Run validation with:

\`\`\`bash
vsa validate --config vsa.yaml
\`\`\`

## Notes

[TODO: Add any additional notes]
EOF
}

# ============================================================================
# Python Fixture Generator
# ============================================================================

create_python_fixture() {
    local path=$1
    local name=$2
    local validity=$3
    
    echo "Generating Python fixture structure..."
    
    # Create directories
    mkdir -p "$path/domain/commands/tasks"
    mkdir -p "$path/domain/queries"
    mkdir -p "$path/domain/events/_versioned"
    mkdir -p "$path/domain/events/_upcasters"
    mkdir -p "$path/infrastructure"
    mkdir -p "$path/slices/create_task"
    
    # Add __init__.py files
    touch "$path/domain/__init__.py"
    touch "$path/domain/commands/__init__.py"
    touch "$path/domain/commands/tasks/__init__.py"
    touch "$path/domain/queries/__init__.py"
    touch "$path/domain/events/__init__.py"
    touch "$path/infrastructure/__init__.py"
    touch "$path/slices/__init__.py"
    touch "$path/slices/create_task/__init__.py"
    
    # vsa.yaml (same as TypeScript but language: python)
    cat > "$path/vsa.yaml" << 'EOF'
version: 2
architecture: "hexagonal-event-sourced-vsa"
language: "python"

domain:
  path: "domain"
  aggregates:
    pattern: "*_aggregate.py"
  events:
    path: "events"
    versioning:
      enabled: true
      format: "simple"

slices:
  path: "slices"
  command:
    must_use: "CommandBus"
    max_lines: 50
    no_business_logic: true

validation:
  architecture:
    enforce_hexagonal: true
  event_sourcing:
    require_event_versioning: true
EOF
    
    # Python aggregate
    cat > "$path/domain/task_aggregate.py" << 'EOF'
from vsa.core import Aggregate, command_handler, event_sourcing_handler
from .commands.tasks.create_task_command import CreateTaskCommand
from .events.task_created_event import TaskCreatedEvent

@Aggregate()
class TaskAggregate:
    def __init__(self):
        self.id = None
        self.title = None
        self.completed = False
    
    @command_handler
    def handle(self, command: CreateTaskCommand) -> None:
        if not command.title:
            raise ValueError('Task title is required')
        
        self.apply(TaskCreatedEvent(
            aggregate_id=command.aggregate_id,
            title=command.title
        ))
    
    @event_sourcing_handler
    def on(self, event: TaskCreatedEvent) -> None:
        self.id = event.aggregate_id
        self.title = event.title
EOF
    
    # Python command
    cat > "$path/domain/commands/tasks/create_task_command.py" << 'EOF'
from dataclasses import dataclass

@dataclass
class CreateTaskCommand:
    aggregate_id: str
    title: str
EOF
    
    # Python event
    cat > "$path/domain/events/task_created_event.py" << 'EOF'
from dataclasses import dataclass
from vsa.core import Event

@Event('TaskCreated', 'v1')
@dataclass
class TaskCreatedEvent:
    aggregate_id: str
    title: str
EOF
    
    # README
    cat > "$path/README.md" << EOF
# Test Fixture: ${FULL_FIXTURE_NAME}

**Language:** Python  
**Validity:** ${VALIDITY}  
**Purpose:** [TODO: Describe what this fixture tests]
EOF
}

# ============================================================================
# Rust Fixture Generator
# ============================================================================

create_rust_fixture() {
    local path=$1
    local name=$2
    local validity=$3
    
    echo "Generating Rust fixture structure..."
    
    # Create directories
    mkdir -p "$path/domain/commands/tasks"
    mkdir -p "$path/domain/events"
    mkdir -p "$path/infrastructure"
    mkdir -p "$path/slices/create_task"
    
    # Cargo.toml
    cat > "$path/Cargo.toml" << 'EOF'
[package]
name = "vsa-test-fixture"
version = "0.1.0"
edition = "2021"

[dependencies]
vsa-core = { path = "../../../vsa-core" }
EOF
    
    # vsa.yaml
    cat > "$path/vsa.yaml" << 'EOF'
version: 2
architecture: "hexagonal-event-sourced-vsa"
language: "rust"

domain:
  path: "domain"
  aggregates:
    pattern: "*_aggregate.rs"
  events:
    versioning:
      enabled: true

slices:
  path: "slices"
  command:
    must_use: "CommandBus"

validation:
  architecture:
    enforce_hexagonal: true
EOF
    
    # Rust aggregate
    cat > "$path/domain/task_aggregate.rs" << 'EOF'
use vsa_core::{Aggregate, CommandHandler, EventSourcingHandler};

#[derive(Aggregate)]
pub struct TaskAggregate {
    id: String,
    title: String,
    completed: bool,
}

impl TaskAggregate {
    #[command_handler]
    pub fn handle(&mut self, command: CreateTaskCommand) -> Result<(), String> {
        if command.title.is_empty() {
            return Err("Task title is required".to_string());
        }
        
        self.apply(TaskCreatedEvent {
            aggregate_id: command.aggregate_id,
            title: command.title,
        });
        
        Ok(())
    }
    
    #[event_sourcing_handler]
    fn on(&mut self, event: TaskCreatedEvent) {
        self.id = event.aggregate_id;
        self.title = event.title;
    }
}
EOF
    
    # README
    cat > "$path/README.md" << EOF
# Test Fixture: ${FULL_FIXTURE_NAME}

**Language:** Rust  
**Validity:** ${VALIDITY}  
**Purpose:** [TODO: Describe what this fixture tests]
EOF
}

