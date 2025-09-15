//! Order processing example demonstrating command handling and event sourcing

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

fn main() {
    println!("Order processing example");

    // Create and process an order
    let mut order = Order::default();

    let events = vec![
        OrderEvent::Created {
            id: "order-123".to_string(),
            customer_id: "customer-456".to_string(),
        },
        OrderEvent::ItemAdded {
            product_id: "product-1".to_string(),
            quantity: 2,
            price: 29.99,
        },
        OrderEvent::ItemAdded {
            product_id: "product-2".to_string(),
            quantity: 1,
            price: 49.99,
        },
        OrderEvent::Confirmed,
        OrderEvent::Shipped {
            tracking_number: "TRK123456".to_string(),
        },
    ];

    for event in &events {
        order.apply_event(event).unwrap();
    }

    println!("Order after processing: {order:?}");
    println!("Order total: ${:.2}", order.total);
    println!("Number of items: {}", order.items.len());
}
