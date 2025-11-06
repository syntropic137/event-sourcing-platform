import { CreateOrderCommand } from '../../domain/commands/CreateOrderCommand';
import { OrderAggregate } from '../../domain/OrderAggregate';

/**
 * ❌ INVALID: This slice contains business logic (should be thin adapter only).
 * Business logic should be in the domain/aggregate, NOT in the slice.
 */
export class CreateOrderCli {
    private aggregate: OrderAggregate;
    private orderHistory: string[] = [];

    constructor() {
        this.aggregate = new OrderAggregate();
    }

    async execute(orderId: string, customerId: string, items: string[], prices: number[]): Promise<void> {
        // ❌ VIOLATION: Business validation in slice (should be in domain)
        if (items.length === 0) {
            throw new Error('Order must have at least one item');
        }

        if (items.length !== prices.length) {
            throw new Error('Items and prices must match');
        }

        // ❌ VIOLATION: Business calculation in slice (should be in domain)
        let totalAmount = 0;
        for (const price of prices) {
            if (price <= 0) {
                throw new Error('Price must be positive');
            }
            totalAmount += price;
        }

        // ❌ VIOLATION: Discount logic in slice (should be in domain)
        if (totalAmount > 100) {
            totalAmount *= 0.9; // 10% discount for orders over $100
        }

        // ❌ VIOLATION: State management in slice
        this.orderHistory.push(orderId);

        // ❌ VIOLATION: Direct aggregate access (should use command bus)
        const command = new CreateOrderCommand(orderId, customerId, totalAmount);
        const event = this.aggregate.createOrder(command);
        this.aggregate.apply(event);

        // ❌ VIOLATION: Complex logging logic
        console.log(`Order ${orderId} created with total $${totalAmount.toFixed(2)}`);
        console.log(`Customer ${customerId} has ${this.orderHistory.length} orders`);
        console.log(`Applied discount: ${totalAmount > 100 ? 'Yes' : 'No'}`);
    }

    // ❌ VIOLATION: Query logic in command slice
    getOrderHistory(): string[] {
        return [...this.orderHistory];
    }
}

