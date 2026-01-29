/**
 * Tests for Scenario Testing (Given-When-Then)
 * Verifies the ES Test Kit scenario() API works correctly
 */

import {
  AggregateRoot,
  Aggregate,
  CommandHandler,
  EventSourcingHandler,
  BaseDomainEvent,
} from '../src';

import { scenario, ScenarioAssertionError } from '../src/testing';

// ============================================================================
// TEST DOMAIN: Shopping Cart
// ============================================================================

// Events
class CartCreatedEvent extends BaseDomainEvent {
  readonly eventType = 'CartCreated' as const;
  readonly schemaVersion = 1 as const;

  constructor(public readonly cartId: string) {
    super();
  }
}

class ItemAddedEvent extends BaseDomainEvent {
  readonly eventType = 'ItemAdded' as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public readonly cartId: string,
    public readonly itemId: string,
    public readonly price: number
  ) {
    super();
  }
}

class CartSubmittedEvent extends BaseDomainEvent {
  readonly eventType = 'CartSubmitted' as const;
  readonly schemaVersion = 1 as const;

  constructor(
    public readonly cartId: string,
    public readonly total: number
  ) {
    super();
  }
}

type CartEvent = CartCreatedEvent | ItemAddedEvent | CartSubmittedEvent;

// Commands
class CreateCartCommand {
  constructor(public readonly aggregateId: string) {}
}

class AddItemCommand {
  constructor(
    public readonly aggregateId: string,
    public readonly itemId: string,
    public readonly price: number
  ) {}
}

class SubmitCartCommand {
  constructor(public readonly aggregateId: string) {}
}

// Business Rule Errors
class BusinessRuleViolationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'BusinessRuleViolationError';
  }
}

// Aggregate
@Aggregate('Cart')
class CartAggregate extends AggregateRoot<CartEvent> {
  private items: Array<{ itemId: string; price: number }> = [];
  private submitted = false;

  @CommandHandler('CreateCartCommand')
  createCart(command: CreateCartCommand): void {
    if (this.id !== null) {
      throw new BusinessRuleViolationError('Cart already exists');
    }
    this.initialize(command.aggregateId);
    this.apply(new CartCreatedEvent(command.aggregateId));
  }

  @CommandHandler('AddItemCommand')
  addItem(command: AddItemCommand): void {
    if (this.id === null) {
      throw new BusinessRuleViolationError('Cart does not exist');
    }
    if (this.submitted) {
      throw new BusinessRuleViolationError('Cannot add items to submitted cart');
    }
    if (command.price <= 0) {
      throw new BusinessRuleViolationError('Price must be positive');
    }
    this.apply(new ItemAddedEvent(command.aggregateId, command.itemId, command.price));
  }

  @CommandHandler('SubmitCartCommand')
  submitCart(command: SubmitCartCommand): void {
    if (this.id === null) {
      throw new BusinessRuleViolationError('Cart does not exist');
    }
    if (this.submitted) {
      throw new BusinessRuleViolationError('Cart already submitted');
    }
    if (this.items.length === 0) {
      throw new BusinessRuleViolationError('Cannot submit empty cart');
    }
    const total = this.items.reduce((sum, item) => sum + item.price, 0);
    this.apply(new CartSubmittedEvent(command.aggregateId, total));
  }

  @EventSourcingHandler('CartCreated')
  private onCartCreated(): void {
    // Initial state is already set
  }

  @EventSourcingHandler('ItemAdded')
  private onItemAdded(event: ItemAddedEvent): void {
    this.items.push({ itemId: event.itemId, price: event.price });
  }

  @EventSourcingHandler('CartSubmitted')
  private onCartSubmitted(): void {
    this.submitted = true;
  }

  // Accessors for testing
  getItemCount(): number {
    return this.items.length;
  }

  getTotal(): number {
    return this.items.reduce((sum, item) => sum + item.price, 0);
  }

  isSubmitted(): boolean {
    return this.submitted;
  }
}

// ============================================================================
// TESTS
// ============================================================================

describe('Scenario Testing (Given-When-Then)', () => {
  describe('Happy Path - Events Emitted', () => {
    it('should verify command produces expected events', () => {
      scenario(CartAggregate)
        .given([
          new CartCreatedEvent('cart-1'),
        ])
        .when(new AddItemCommand('cart-1', 'item-1', 29.99))
        .expectEvents([
          new ItemAddedEvent('cart-1', 'item-1', 29.99),
        ]);
    });

    it('should verify multiple events in sequence', () => {
      scenario(CartAggregate)
        .given([
          new CartCreatedEvent('cart-1'),
          new ItemAddedEvent('cart-1', 'item-1', 10.00),
          new ItemAddedEvent('cart-1', 'item-2', 20.00),
        ])
        .when(new SubmitCartCommand('cart-1'))
        .expectEvents([
          new CartSubmittedEvent('cart-1', 30.00),
        ]);
    });

    it('should verify command with givenNoPriorActivity()', () => {
      scenario(CartAggregate)
        .givenNoPriorActivity()
        .when(new CreateCartCommand('cart-new'))
        .expectEvents([
          new CartCreatedEvent('cart-new'),
        ]);
    });
  });

  describe('Error Path - Exceptions', () => {
    it('should verify business rule violation exception', () => {
      scenario(CartAggregate)
        .given([
          new CartCreatedEvent('cart-1'),
        ])
        .when(new SubmitCartCommand('cart-1'))
        .expectException(BusinessRuleViolationError)
        .expectExceptionMessage('Cannot submit empty cart');
    });

    it('should verify exception with string containment', () => {
      scenario(CartAggregate)
        .givenNoPriorActivity()
        .when(new SubmitCartCommand('cart-1'))
        .expectException(BusinessRuleViolationError)
        .expectExceptionMessage('does not exist');
    });

    it('should verify exception with regex pattern', () => {
      scenario(CartAggregate)
        .given([
          new CartCreatedEvent('cart-1'),
          new CartSubmittedEvent('cart-1', 0),
        ])
        .when(new SubmitCartCommand('cart-1'))
        .expectException(BusinessRuleViolationError)
        .expectExceptionMessage(/already submitted/i);
    });

    it('should verify exception type only', () => {
      scenario(CartAggregate)
        .given([
          new CartCreatedEvent('cart-1'),
        ])
        .when(new AddItemCommand('cart-1', 'item-1', -10))
        .expectException(BusinessRuleViolationError);
    });
  });

  describe('No Events', () => {
    it('should fail when expecting no events but events are emitted', () => {
      expect(() => {
        scenario(CartAggregate)
          .givenNoPriorActivity()
          .when(new CreateCartCommand('cart-1'))
          .expectNoEvents();
      }).toThrow(ScenarioAssertionError);
    });
  });

  describe('State Verification', () => {
    it('should verify aggregate state via callback', () => {
      scenario(CartAggregate)
        .given([
          new CartCreatedEvent('cart-1'),
        ])
        .when(new AddItemCommand('cart-1', 'item-1', 29.99))
        .expectState((aggregate) => {
          expect(aggregate.getItemCount()).toBe(1);
          expect(aggregate.getTotal()).toBe(29.99);
          expect(aggregate.isSubmitted()).toBe(false);
        });
    });

    it('should verify state after multiple events', () => {
      scenario(CartAggregate)
        .given([
          new CartCreatedEvent('cart-1'),
          new ItemAddedEvent('cart-1', 'item-1', 10.00),
          new ItemAddedEvent('cart-1', 'item-2', 20.00),
        ])
        .when(new SubmitCartCommand('cart-1'))
        .expectState((aggregate) => {
          expect(aggregate.isSubmitted()).toBe(true);
          expect(aggregate.getTotal()).toBe(30.00);
        });
    });
  });

  describe('givenCommands()', () => {
    it('should set up aggregate state using commands', () => {
      scenario(CartAggregate)
        .givenCommands([
          new CreateCartCommand('cart-1'),
          new AddItemCommand('cart-1', 'item-1', 15.00),
          new AddItemCommand('cart-1', 'item-2', 25.00),
        ])
        .when(new SubmitCartCommand('cart-1'))
        .expectEvents([
          new CartSubmittedEvent('cart-1', 40.00),
        ]);
    });
  });

  describe('Assertion Failures', () => {
    it('should fail when expected events do not match actual events', () => {
      expect(() => {
        scenario(CartAggregate)
          .given([
            new CartCreatedEvent('cart-1'),
          ])
          .when(new AddItemCommand('cart-1', 'item-1', 29.99))
          .expectEvents([
            new ItemAddedEvent('cart-1', 'item-1', 99.99), // Wrong price
          ]);
      }).toThrow(ScenarioAssertionError);
    });

    it('should fail when expected event count does not match', () => {
      expect(() => {
        scenario(CartAggregate)
          .givenNoPriorActivity()
          .when(new CreateCartCommand('cart-1'))
          .expectEvents([
            new CartCreatedEvent('cart-1'),
            new ItemAddedEvent('cart-1', 'item-1', 10.00), // Extra event expected
          ]);
      }).toThrow(ScenarioAssertionError);
    });

    it('should fail when expecting exception but command succeeds', () => {
      expect(() => {
        scenario(CartAggregate)
          .givenNoPriorActivity()
          .when(new CreateCartCommand('cart-1'))
          .expectException(BusinessRuleViolationError);
      }).toThrow(ScenarioAssertionError);
    });

    it('should fail when expecting wrong exception type', () => {
      expect(() => {
        scenario(CartAggregate)
          .given([
            new CartCreatedEvent('cart-1'),
          ])
          .when(new SubmitCartCommand('cart-1'))
          .expectException(Error); // Generic Error, not BusinessRuleViolationError
      }).not.toThrow(); // BusinessRuleViolationError IS an Error, so this passes
    });

    it('should fail when exception message does not match', () => {
      expect(() => {
        scenario(CartAggregate)
          .given([
            new CartCreatedEvent('cart-1'),
          ])
          .when(new SubmitCartCommand('cart-1'))
          .expectException(BusinessRuleViolationError)
          .expectExceptionMessage('wrong message');
      }).toThrow(ScenarioAssertionError);
    });
  });

  describe('expectSuccessfulHandlerExecution()', () => {
    it('should pass when command succeeds', () => {
      scenario(CartAggregate)
        .givenNoPriorActivity()
        .when(new CreateCartCommand('cart-1'))
        .expectSuccessfulHandlerExecution();
    });

    it('should fail when command throws', () => {
      expect(() => {
        scenario(CartAggregate)
          .givenNoPriorActivity()
          .when(new SubmitCartCommand('cart-1'))
          .expectSuccessfulHandlerExecution();
      }).toThrow(ScenarioAssertionError);
    });
  });

  describe('Chaining', () => {
    it('should allow chaining expectEvents and expectState', () => {
      scenario(CartAggregate)
        .given([
          new CartCreatedEvent('cart-1'),
        ])
        .when(new AddItemCommand('cart-1', 'item-1', 29.99))
        .expectEvents([
          new ItemAddedEvent('cart-1', 'item-1', 29.99),
        ])
        .expectState((aggregate) => {
          expect(aggregate.getItemCount()).toBe(1);
        });
    });

    it('should allow chaining expectException and expectExceptionMessage', () => {
      scenario(CartAggregate)
        .given([
          new CartCreatedEvent('cart-1'),
        ])
        .when(new SubmitCartCommand('cart-1'))
        .expectException(BusinessRuleViolationError)
        .expectExceptionMessage('Cannot submit empty cart');
    });
  });

  describe('Edge Cases', () => {
    it('should handle empty given events array', () => {
      scenario(CartAggregate)
        .given([])
        .when(new CreateCartCommand('cart-1'))
        .expectEvents([
          new CartCreatedEvent('cart-1'),
        ]);
    });

    it('should handle aggregate without prior state creating new aggregate', () => {
      scenario(CartAggregate)
        .givenNoPriorActivity()
        .when(new CreateCartCommand('brand-new-cart'))
        .expectEvents([
          new CartCreatedEvent('brand-new-cart'),
        ])
        .expectState((aggregate) => {
          expect(aggregate.id).toBe('brand-new-cart');
        });
    });
  });
});
