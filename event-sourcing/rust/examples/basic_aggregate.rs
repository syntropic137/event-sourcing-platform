//! Basic aggregate example demonstrating event sourcing patterns

use event_sourcing_rust::prelude::*;
use serde::{Deserialize, Serialize};

/// Example user aggregate
#[derive(Debug, Clone, Default)]
struct User {
    id: Option<String>,
    name: String,
    email: String,
    is_active: bool,
    version: u64,
}

/// Events that can happen to a user
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum UserEvent {
    Created {
        id: String,
        name: String,
        email: String,
    },
    NameChanged {
        name: String,
    },
    EmailChanged {
        email: String,
    },
    Activated,
    Deactivated,
}

impl DomainEvent for UserEvent {
    fn event_type(&self) -> &'static str {
        match self {
            UserEvent::Created { .. } => "UserCreated",
            UserEvent::NameChanged { .. } => "UserNameChanged",
            UserEvent::EmailChanged { .. } => "UserEmailChanged",
            UserEvent::Activated => "UserActivated",
            UserEvent::Deactivated => "UserDeactivated",
        }
    }
}

impl Aggregate for User {
    type Event = UserEvent;
    type Error = Error;

    fn aggregate_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn version(&self) -> u64 {
        self.version
    }

    fn apply_event(&mut self, event: &Self::Event) -> Result<()> {
        match event {
            UserEvent::Created { id, name, email } => {
                self.id = Some(id.clone());
                self.name = name.clone();
                self.email = email.clone();
                self.is_active = false;
                self.version += 1;
            }
            UserEvent::NameChanged { name } => {
                self.name = name.clone();
                self.version += 1;
            }
            UserEvent::EmailChanged { email } => {
                self.email = email.clone();
                self.version += 1;
            }
            UserEvent::Activated => {
                self.is_active = true;
                self.version += 1;
            }
            UserEvent::Deactivated => {
                self.is_active = false;
                self.version += 1;
            }
        }
        Ok(())
    }
}

fn main() {
    println!("Basic aggregate example");

    // Create a new user
    let mut user = User::default();

    // Apply some events
    let events = vec![
        UserEvent::Created {
            id: "user-123".to_string(),
            name: "John Doe".to_string(),
            email: "john@example.com".to_string(),
        },
        UserEvent::Activated,
        UserEvent::NameChanged {
            name: "John Smith".to_string(),
        },
    ];

    for event in &events {
        user.apply_event(event).unwrap();
    }

    println!("User after applying events: {user:?}");
    println!("User is active: {}", user.is_active);
    println!("User name: {}", user.name);
}
