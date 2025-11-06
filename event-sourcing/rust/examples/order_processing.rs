//! Order processing example demonstrating command handling and event sourcing
//!
//! This example demonstrates ADR-004 compliance:
//! - Command handlers integrated in aggregates via AggregateRoot trait
//! - Business validation in handle_command()
//! - State updates only in apply_event()
//! - Commands processed through validation before events are applied

use async_trait::async_trait;
use event_sourcing_rust::prelude::*;
use serde::{Deserialize, Serialize};

/// Order aggregate
#[derive(Debug, Clone, Default)]
struct Order {
    id: Option<String>,
    customer_id: String,
    items: Vec<OrderItem>,
    status: OrderStatus,
    total: f64,
    version: u64,
}

#[derive(Debug, Clone)]
struct OrderItem {
    product_id: String,
    quantity: u32,
    price: f64,
}

#[derive(Debug, Clone, Default)]
enum OrderStatus {
    #[default]
    Draft,
    Confirmed,
    Shipped,
    Delivered,
    Cancelled,
}

/// Order events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum OrderEvent {
    Created {
        id: String,
        customer_id: String,
    },
    ItemAdded {
        product_id: String,
        quantity: u32,
        price: f64,
    },
    ItemRemoved {
        product_id: String,
    },
    Confirmed,
    Shipped {
        tracking_number: String,
    },
    Delivered,
    Cancelled {
        reason: String,
    },
}

impl DomainEvent for OrderEvent {
    fn event_type(&self) -> &'static str {
        match self {
            OrderEvent::Created { .. } => "OrderCreated",
            OrderEvent::ItemAdded { .. } => "OrderItemAdded",
            OrderEvent::ItemRemoved { .. } => "OrderItemRemoved",
            OrderEvent::Confirmed => "OrderConfirmed",
            OrderEvent::Shipped { .. } => "OrderShipped",
            OrderEvent::Delivered => "OrderDelivered",
            OrderEvent::Cancelled { .. } => "OrderCancelled",
        }
    }
}

impl Aggregate for Order {
    type Event = OrderEvent;
    type Error = Error;

    fn aggregate_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn version(&self) -> u64 {
        self.version
    }

    fn apply_event(&mut self, event: &Self::Event) -> Result<()> {
        match event {
            OrderEvent::Created { id, customer_id } => {
                self.id = Some(id.clone());
                self.customer_id = customer_id.clone();
                self.status = OrderStatus::Draft;
                self.version += 1;
            }
            OrderEvent::ItemAdded {
                product_id,
                quantity,
                price,
            } => {
                self.items.push(OrderItem {
                    product_id: product_id.clone(),
                    quantity: *quantity,
                    price: *price,
                });
                self.recalculate_total();
                self.version += 1;
            }
            OrderEvent::ItemRemoved { product_id } => {
                self.items.retain(|item| item.product_id != *product_id);
                self.recalculate_total();
                self.version += 1;
            }
            OrderEvent::Confirmed => {
                self.status = OrderStatus::Confirmed;
                self.version += 1;
            }
            OrderEvent::Shipped { .. } => {
                self.status = OrderStatus::Shipped;
                self.version += 1;
            }
            OrderEvent::Delivered => {
                self.status = OrderStatus::Delivered;
                self.version += 1;
            }
            OrderEvent::Cancelled { .. } => {
                self.status = OrderStatus::Cancelled;
                self.version += 1;
            }
        }
        Ok(())
    }
}

//=============================================================================
// ADR-004: Command Handlers in Aggregates
//=============================================================================

#[async_trait]
impl AggregateRoot for Order {
    type Command = OrderCommand;

    /// Handle commands with business logic validation
    ///
    /// This method implements ADR-004 pattern: commands are validated here,
    /// and events are returned to be applied. State updates happen only in
    /// apply_event(), ensuring a clear separation of concerns.
    async fn handle_command(&self, command: Self::Command) -> Result<Vec<Self::Event>> {
        match command {
            // CREATE ORDER - Validate order doesn't exist
            OrderCommand::CreateOrder { id, customer_id } => {
                if self.id.is_some() {
                    return Err(Error::invalid_command("Order already exists"));
                }
                if customer_id.is_empty() {
                    return Err(Error::invalid_command("Customer ID is required"));
                }
                Ok(vec![OrderEvent::Created { id, customer_id }])
            }

            // ADD ITEM - Validate order exists and is in Draft status
            OrderCommand::AddItem {
                product_id,
                quantity,
                price,
            } => {
                if self.id.is_none() {
                    return Err(Error::invalid_command(
                        "Cannot add item to non-existent order",
                    ));
                }
                if !matches!(self.status, OrderStatus::Draft) {
                    return Err(Error::invalid_command(
                        "Cannot add items to confirmed order",
                    ));
                }
                if quantity == 0 {
                    return Err(Error::invalid_command("Quantity must be greater than 0"));
                }
                if price < 0.0 {
                    return Err(Error::invalid_command("Price cannot be negative"));
                }
                Ok(vec![OrderEvent::ItemAdded {
                    product_id,
                    quantity,
                    price,
                }])
            }

            // REMOVE ITEM - Validate order exists and is in Draft status
            OrderCommand::RemoveItem { product_id } => {
                if self.id.is_none() {
                    return Err(Error::invalid_command(
                        "Cannot remove item from non-existent order",
                    ));
                }
                if !matches!(self.status, OrderStatus::Draft) {
                    return Err(Error::invalid_command(
                        "Cannot remove items from confirmed order",
                    ));
                }
                if !self.items.iter().any(|item| item.product_id == product_id) {
                    return Err(Error::invalid_command("Item not found in order"));
                }
                Ok(vec![OrderEvent::ItemRemoved { product_id }])
            }

            // CONFIRM ORDER - Validate order has items
            OrderCommand::ConfirmOrder => {
                if self.id.is_none() {
                    return Err(Error::invalid_command("Cannot confirm non-existent order"));
                }
                if !matches!(self.status, OrderStatus::Draft) {
                    return Err(Error::invalid_command("Order is not in Draft status"));
                }
                if self.items.is_empty() {
                    return Err(Error::invalid_command("Cannot confirm order with no items"));
                }
                Ok(vec![OrderEvent::Confirmed])
            }

            // SHIP ORDER - Validate order is confirmed
            OrderCommand::ShipOrder { tracking_number } => {
                if self.id.is_none() {
                    return Err(Error::invalid_command("Cannot ship non-existent order"));
                }
                if !matches!(self.status, OrderStatus::Confirmed) {
                    return Err(Error::invalid_command(
                        "Order must be confirmed before shipping",
                    ));
                }
                if tracking_number.is_empty() {
                    return Err(Error::invalid_command("Tracking number is required"));
                }
                Ok(vec![OrderEvent::Shipped { tracking_number }])
            }

            // DELIVER ORDER - Validate order is shipped
            OrderCommand::DeliverOrder => {
                if self.id.is_none() {
                    return Err(Error::invalid_command("Cannot deliver non-existent order"));
                }
                if !matches!(self.status, OrderStatus::Shipped) {
                    return Err(Error::invalid_command(
                        "Order must be shipped before delivery",
                    ));
                }
                Ok(vec![OrderEvent::Delivered])
            }

            // CANCEL ORDER - Validate order is not delivered
            OrderCommand::CancelOrder { reason } => {
                if self.id.is_none() {
                    return Err(Error::invalid_command("Cannot cancel non-existent order"));
                }
                if matches!(self.status, OrderStatus::Delivered) {
                    return Err(Error::invalid_command("Cannot cancel delivered order"));
                }
                if matches!(self.status, OrderStatus::Cancelled) {
                    return Err(Error::invalid_command("Order is already cancelled"));
                }
                if reason.is_empty() {
                    return Err(Error::invalid_command("Cancellation reason is required"));
                }
                Ok(vec![OrderEvent::Cancelled { reason }])
            }
        }
    }
}

//=============================================================================
// Helper Methods
//=============================================================================

impl Order {
    fn recalculate_total(&mut self) {
        self.total = self
            .items
            .iter()
            .map(|item| item.price * item.quantity as f64)
            .sum();
    }
}

/// Order commands
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum OrderCommand {
    CreateOrder {
        id: String,
        customer_id: String,
    },
    AddItem {
        product_id: String,
        quantity: u32,
        price: f64,
    },
    RemoveItem {
        product_id: String,
    },
    ConfirmOrder,
    ShipOrder {
        tracking_number: String,
    },
    DeliverOrder,
    CancelOrder {
        reason: String,
    },
}

impl Command for OrderCommand {}

#[tokio::main]
async fn main() {
    println!("🛒 Order Processing Example - ADR-004 Compliant");
    println!("================================================\n");

    let mut order = Order::default();

    // Step 1: Create Order
    println!("📦 Step 1: Create Order");
    let create_cmd = OrderCommand::CreateOrder {
        id: "order-123".to_string(),
        customer_id: "customer-456".to_string(),
    };
    let events = order.handle_command(create_cmd).await.unwrap();
    for event in &events {
        order.apply_event(event).unwrap();
    }
    println!("   ✓ Order created: {}", order.id.as_ref().unwrap());

    // Step 2: Add Items
    println!("\n📝 Step 2: Add Items");
    let add_item1 = OrderCommand::AddItem {
        product_id: "product-1".to_string(),
        quantity: 2,
        price: 29.99,
    };
    let events = order.handle_command(add_item1).await.unwrap();
    for event in &events {
        order.apply_event(event).unwrap();
    }
    println!("   ✓ Added 2x product-1 @ $29.99");

    let add_item2 = OrderCommand::AddItem {
        product_id: "product-2".to_string(),
        quantity: 1,
        price: 49.99,
    };
    let events = order.handle_command(add_item2).await.unwrap();
    for event in &events {
        order.apply_event(event).unwrap();
    }
    println!("   ✓ Added 1x product-2 @ $49.99");
    println!("   Total: ${:.2}", order.total);

    // Step 3: Confirm Order
    println!("\n✅ Step 3: Confirm Order");
    let confirm_cmd = OrderCommand::ConfirmOrder;
    let events = order.handle_command(confirm_cmd).await.unwrap();
    for event in &events {
        order.apply_event(event).unwrap();
    }
    println!("   ✓ Order confirmed (Status: {:?})", order.status);

    // Step 4: Ship Order
    println!("\n📮 Step 4: Ship Order");
    let ship_cmd = OrderCommand::ShipOrder {
        tracking_number: "TRK123456789".to_string(),
    };
    let events = order.handle_command(ship_cmd).await.unwrap();
    for event in &events {
        order.apply_event(event).unwrap();
    }
    println!("   ✓ Order shipped (Tracking: TRK123456789)");

    // Step 5: Deliver Order
    println!("\n🎉 Step 5: Deliver Order");
    let deliver_cmd = OrderCommand::DeliverOrder;
    let events = order.handle_command(deliver_cmd).await.unwrap();
    for event in &events {
        order.apply_event(event).unwrap();
    }
    println!("   ✓ Order delivered!");

    // Final Summary
    println!("\n📊 Final Order Summary:");
    println!("   Order ID: {}", order.id.as_ref().unwrap());
    println!("   Customer: {}", order.customer_id);
    println!("   Status: {:?}", order.status);
    println!("   Items: {}", order.items.len());
    println!("   Total: ${:.2}", order.total);

    // Demonstrate Validation
    println!("\n🔒 Demonstrating Business Rule Validation:");
    println!("   Attempting to add item to delivered order...");
    let invalid_cmd = OrderCommand::AddItem {
        product_id: "product-3".to_string(),
        quantity: 1,
        price: 19.99,
    };
    match order.handle_command(invalid_cmd).await {
        Ok(_) => println!("   ❌ ERROR: Should have been rejected!"),
        Err(e) => println!("   ✓ Correctly rejected: {e:?}"),
    }

    println!("\n✅ ADR-004 Pattern Demonstrated:");
    println!("   • Commands validated in handle_command()");
    println!("   • Events applied in apply_event()");
    println!("   • Business rules enforced");
    println!("   • Invalid operations prevented");
}
