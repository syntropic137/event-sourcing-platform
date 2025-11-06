/**
 * Tests for Command Handler Infrastructure
 * Verifies that @CommandHandler decorator and handleCommand() work correctly
 */

import {
  AggregateRoot,
  Aggregate,
  CommandHandler,
  EventSourcingHandler,
  BaseDomainEvent,
} from '../src';

// Test Events
class TestCreatedEvent extends BaseDomainEvent {
  readonly eventType = 'TestCreated';
  readonly schemaVersion = 1;

  constructor(
    public readonly id: string,
    public readonly name: string
  ) {
    super();
  }
}

class TestUpdatedEvent extends BaseDomainEvent {
  readonly eventType = 'TestUpdated';
  readonly schemaVersion = 1;

  constructor(
    public readonly id: string,
    public readonly newName: string
  ) {
    super();
  }
}

// Test Commands
class CreateTestCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly name: string
  ) {}
}

class UpdateTestCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly newName: string
  ) {}
}

class UnknownCommand {
  constructor(public readonly aggregateId: string) {}
}

// Test Aggregate
@Aggregate('TestAggregate')
class TestAggregate extends AggregateRoot<TestCreatedEvent | TestUpdatedEvent> {
  private name: string | null = null;
  private updateCount: number = 0;

  @CommandHandler('CreateTestCommand')
  createTest(command: CreateTestCommand): void {
    // Validation
    if (!command.name || command.name.trim() === '') {
      throw new Error('Name is required');
    }

    if (this.id !== null) {
      throw new Error('Aggregate already created');
    }

    // Initialize aggregate (required before raising events)
    this.initialize(command.aggregateId);

    // Apply event
    this.apply(new TestCreatedEvent(command.aggregateId, command.name));
  }

  @CommandHandler('UpdateTestCommand')
  updateTest(command: UpdateTestCommand): void {
    // Validation
    if (this.id === null) {
      throw new Error('Aggregate not created yet');
    }

    if (!command.newName || command.newName.trim() === '') {
      throw new Error('New name is required');
    }

    // Apply event
    this.apply(new TestUpdatedEvent(command.aggregateId, command.newName));
  }

  @EventSourcingHandler('TestCreated')
  private onTestCreated(event: TestCreatedEvent): void {
    // State update only - initialization happens in command handler
    this.name = event.name;
  }

  @EventSourcingHandler('TestUpdated')
  private onTestUpdated(event: TestUpdatedEvent): void {
    this.name = event.newName;
    this.updateCount++;
  }

  // Accessors for testing
  getName(): string | null {
    return this.name;
  }

  getUpdateCount(): number {
    return this.updateCount;
  }
}

describe('Command Handler Infrastructure', () => {
  describe('Command Dispatching', () => {
    it('should dispatch CreateTestCommand to decorated handler method', () => {
      // Arrange
      const aggregate = new TestAggregate();
      const command = new CreateTestCommand('test-123', 'Test Name');

      // Act
      (aggregate as any).handleCommand(command);

      // Assert
      expect(aggregate.id).toBe('test-123');
      expect(aggregate.getName()).toBe('Test Name');
      expect(aggregate.getUncommittedEvents()).toHaveLength(1);

      const events = aggregate.getUncommittedEvents();
      expect(events[0].event.eventType).toBe('TestCreated');
      expect((events[0].event as TestCreatedEvent).name).toBe('Test Name');
    });

    it('should dispatch UpdateTestCommand to decorated handler method', () => {
      // Arrange
      const aggregate = new TestAggregate();
      const createCommand = new CreateTestCommand('test-456', 'Original Name');
      (aggregate as any).handleCommand(createCommand);
      aggregate.markEventsAsCommitted();

      const updateCommand = new UpdateTestCommand('test-456', 'Updated Name');

      // Act
      (aggregate as any).handleCommand(updateCommand);

      // Assert
      expect(aggregate.getName()).toBe('Updated Name');
      expect(aggregate.getUpdateCount()).toBe(1);
      expect(aggregate.getUncommittedEvents()).toHaveLength(1);

      const events = aggregate.getUncommittedEvents();
      expect(events[0].event.eventType).toBe('TestUpdated');
    });

    it('should handle multiple commands on same aggregate', () => {
      // Arrange
      const aggregate = new TestAggregate();

      // Act - Create
      (aggregate as any).handleCommand(new CreateTestCommand('test-789', 'First'));
      aggregate.markEventsAsCommitted();

      // Act - Update multiple times
      (aggregate as any).handleCommand(new UpdateTestCommand('test-789', 'Second'));
      (aggregate as any).handleCommand(new UpdateTestCommand('test-789', 'Third'));

      // Assert
      expect(aggregate.getName()).toBe('Third');
      expect(aggregate.getUpdateCount()).toBe(2);
      expect(aggregate.getUncommittedEvents()).toHaveLength(2);
    });
  });

  describe('Command Validation', () => {
    it('should enforce business rules in command handler', () => {
      // Arrange
      const aggregate = new TestAggregate();
      const command = new CreateTestCommand('test-invalid', '');

      // Act & Assert
      expect(() => {
        (aggregate as any).handleCommand(command);
      }).toThrow('Name is required');

      // No events should be emitted
      expect(aggregate.getUncommittedEvents()).toHaveLength(0);
    });

    it('should validate aggregate state in command handler', () => {
      // Arrange
      const aggregate = new TestAggregate();
      const command = new UpdateTestCommand('test-123', 'New Name');

      // Act & Assert
      expect(() => {
        (aggregate as any).handleCommand(command);
      }).toThrow('Aggregate not created yet');
    });

    it('should prevent duplicate creation', () => {
      // Arrange
      const aggregate = new TestAggregate();
      (aggregate as any).handleCommand(new CreateTestCommand('test-123', 'First'));

      const secondCommand = new CreateTestCommand('test-123', 'Second');

      // Act & Assert
      expect(() => {
        (aggregate as any).handleCommand(secondCommand);
      }).toThrow('Aggregate already created');
    });
  });

  describe('Error Handling', () => {
    it('should throw error for command without handler', () => {
      // Arrange
      const aggregate = new TestAggregate();
      const command = new UnknownCommand('test-123');

      // Act & Assert
      expect(() => {
        (aggregate as any).handleCommand(command);
      }).toThrow('No @CommandHandler found for command type: UnknownCommand');
    });

    it('should include aggregate type in error message', () => {
      // Arrange
      const aggregate = new TestAggregate();
      const command = new UnknownCommand('test-123');

      // Act & Assert
      expect(() => {
        (aggregate as any).handleCommand(command);
      }).toThrow(/TestAggregate/);
    });
  });

  describe('Event Emission', () => {
    it('should emit events with correct metadata', () => {
      // Arrange
      const aggregate = new TestAggregate();
      const command = new CreateTestCommand('test-metadata', 'Test');

      // Act
      (aggregate as any).handleCommand(command);

      // Assert
      const envelopes = aggregate.getUncommittedEvents();
      expect(envelopes).toHaveLength(1);

      const envelope = envelopes[0];
      expect(envelope.metadata.aggregateId).toBe('test-metadata');
      expect(envelope.metadata.aggregateType).toBe('TestAggregate');
      expect(envelope.metadata.aggregateNonce).toBe(1);
    });

    it('should increment version for each event', () => {
      // Arrange
      const aggregate = new TestAggregate();
      (aggregate as any).handleCommand(new CreateTestCommand('test-version', 'First'));
      aggregate.markEventsAsCommitted();

      // Act
      (aggregate as any).handleCommand(new UpdateTestCommand('test-version', 'Second'));
      (aggregate as any).handleCommand(new UpdateTestCommand('test-version', 'Third'));

      // Assert
      const envelopes = aggregate.getUncommittedEvents();
      expect(envelopes[0].metadata.aggregateNonce).toBe(2);
      expect(envelopes[1].metadata.aggregateNonce).toBe(3);
      expect(aggregate.version).toBe(3);
    });
  });

  describe('Integration with Event Sourcing Handlers', () => {
    it('should apply events through @EventSourcingHandler', () => {
      // Arrange
      const aggregate = new TestAggregate();
      const command = new CreateTestCommand('test-integration', 'Integration Test');

      // Act
      (aggregate as any).handleCommand(command);

      // Assert - State updated through event handler
      expect(aggregate.getName()).toBe('Integration Test');
      expect(aggregate.id).toBe('test-integration');
    });

    it('should support rehydration after command handling', () => {
      // Arrange - Create and handle commands
      const aggregate1 = new TestAggregate();
      (aggregate1 as any).handleCommand(new CreateTestCommand('test-rehydrate', 'Original'));
      (aggregate1 as any).handleCommand(new UpdateTestCommand('test-rehydrate', 'Updated'));

      const events = aggregate1.getUncommittedEvents();

      // Act - Rehydrate into new instance
      const aggregate2 = new TestAggregate();
      aggregate2.rehydrate(events);

      // Assert
      expect(aggregate2.id).toBe('test-rehydrate');
      expect(aggregate2.getName()).toBe('Updated');
      expect(aggregate2.getUpdateCount()).toBe(1);
      expect(aggregate2.version).toBe(2);
    });
  });
});
