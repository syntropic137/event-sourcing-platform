/**
 * Order Aggregate Tests - Using Given-When-Then Scenario Testing
 *
 * This demonstrates the ES Test Kit's scenario() API for testing
 * event-sourced aggregates with the Given-When-Then pattern.
 *
 * See: ADR-015 ES Test Kit Architecture
 */

import { scenario } from '@neuralempowerment/event-sourcing-typescript/testing';
import { OrderAggregate, OrderStatus } from '../src/domain/OrderAggregate';
import { SubmitOrderCommand } from '../src/domain/commands/SubmitOrderCommand';
import { CancelOrderCommand } from '../src/domain/commands/CancelOrderCommand';
import { OrderSubmittedEvent } from '../src/domain/events/OrderSubmittedEvent';
import { OrderCancelledEvent } from '../src/domain/events/OrderCancelledEvent';

describe('OrderAggregate', () => {
  describe('Submit Order', () => {
    it('should emit OrderSubmittedEvent when submitting a new order', () => {
      // Given: No prior activity (new order)
      // When: Submit order command
      // Then: OrderSubmittedEvent is emitted
      scenario(OrderAggregate)
        .givenNoPriorActivity()
        .when(new SubmitOrderCommand('order-123', 'order-123', 'customer-456'))
        .expectEvents([
          new OrderSubmittedEvent('order-123', 'customer-456'),
        ]);
    });

    it('should update aggregate state to Submitted', () => {
      scenario(OrderAggregate)
        .givenNoPriorActivity()
        .when(new SubmitOrderCommand('order-123', 'order-123', 'customer-456'))
        .expectState((aggregate) => {
          expect(aggregate.getStatus()).toBe(OrderStatus.Submitted);
        });
    });

    it('should reject submitting an already submitted order', () => {
      // Given: Order was already submitted
      // When: Try to submit again
      // Then: Error is thrown
      scenario(OrderAggregate)
        .given([
          new OrderSubmittedEvent('order-123', 'customer-456'),
        ])
        .when(new SubmitOrderCommand('order-123', 'order-123', 'customer-789'))
        .expectException(Error)
        .expectExceptionMessage("Cannot submit order: Order is in 'Submitted' state");
    });

    it('should reject submitting a cancelled order', () => {
      scenario(OrderAggregate)
        .given([
          new OrderSubmittedEvent('order-123', 'customer-456'),
          new OrderCancelledEvent('Changed my mind'),
        ])
        .when(new SubmitOrderCommand('order-123', 'order-123', 'customer-789'))
        .expectException(Error)
        .expectExceptionMessage("Cannot submit order: Order is in 'Cancelled' state");
    });
  });

  describe('Cancel Order', () => {
    it('should emit OrderCancelledEvent when cancelling a submitted order', () => {
      // Given: Order was submitted
      // When: Cancel order command
      // Then: OrderCancelledEvent is emitted
      scenario(OrderAggregate)
        .given([
          new OrderSubmittedEvent('order-123', 'customer-456'),
        ])
        .when(new CancelOrderCommand('order-123', 'Customer requested cancellation'))
        .expectEvents([
          new OrderCancelledEvent('Customer requested cancellation'),
        ]);
    });

    it('should update aggregate state to Cancelled', () => {
      scenario(OrderAggregate)
        .given([
          new OrderSubmittedEvent('order-123', 'customer-456'),
        ])
        .when(new CancelOrderCommand('order-123', 'No longer needed'))
        .expectState((aggregate) => {
          expect(aggregate.getStatus()).toBe(OrderStatus.Cancelled);
        });
    });

    it('should reject cancelling a new (not yet submitted) order', () => {
      // Given: No prior activity (order not submitted)
      // When: Try to cancel
      // Then: Error is thrown
      scenario(OrderAggregate)
        .givenNoPriorActivity()
        .when(new CancelOrderCommand('order-123', 'Changed my mind'))
        .expectException(Error)
        .expectExceptionMessage("Cannot cancel order: Order is in 'New' state");
    });

    it('should reject cancelling an already cancelled order', () => {
      scenario(OrderAggregate)
        .given([
          new OrderSubmittedEvent('order-123', 'customer-456'),
          new OrderCancelledEvent('First cancellation'),
        ])
        .when(new CancelOrderCommand('order-123', 'Second cancellation attempt'))
        .expectException(Error)
        .expectExceptionMessage("Cannot cancel order: Order is in 'Cancelled' state");
    });
  });

  describe('Order Lifecycle', () => {
    it('should support complete submit-then-cancel lifecycle', () => {
      // Using givenCommands to set up state via commands instead of events
      scenario(OrderAggregate)
        .givenCommands([
          new SubmitOrderCommand('order-lifecycle', 'order-lifecycle', 'customer-999'),
        ])
        .when(new CancelOrderCommand('order-lifecycle', 'Lifecycle test cancellation'))
        .expectEvents([
          new OrderCancelledEvent('Lifecycle test cancellation'),
        ])
        .expectState((aggregate) => {
          expect(aggregate.getStatus()).toBe(OrderStatus.Cancelled);
        });
    });
  });
});
