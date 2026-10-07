//! Basic aggregate example demonstrating event sourcing patterns
//!
//! This example demonstrates ADR-004 compliance:
//! - Command handlers integrated in aggregates via AggregateRoot trait
//! - Business validation in handle_command()
//! - State updates only in apply_event()

use async_trait::async_trait;
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

// Events that can happen to a user. Each event is a struct whose fields are
// the stored JSON payload; event type and version are metadata (ADR-027).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserCreated {
    id: String,
    name: String,
    email: String,
}
impl EventSchema for UserCreated {
    const EVENT_TYPE: &'static str = "UserCreated";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserNameChanged {
    name: String,
}
impl EventSchema for UserNameChanged {
    const EVENT_TYPE: &'static str = "UserNameChanged";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserEmailChanged {
    email: String,
}
impl EventSchema for UserEmailChanged {
    const EVENT_TYPE: &'static str = "UserEmailChanged";
}

// An event without data is an empty struct (payload `{}`), never a unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserActivated {}
impl EventSchema for UserActivated {
    const EVENT_TYPE: &'static str = "UserActivated";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserDeactivated {}
impl EventSchema for UserDeactivated {
    const EVENT_TYPE: &'static str = "UserDeactivated";
}

event_sourcing_rust::event_enum! {
    /// Events of the User aggregate
    #[derive(Debug, Clone)]
    enum UserEvent {
        Created(UserCreated),
        NameChanged(UserNameChanged),
        EmailChanged(UserEmailChanged),
        Activated(UserActivated),
        Deactivated(UserDeactivated),
    }
}

impl Aggregate for User {
    type Event = UserEvent;
    type Error = Error;
    const AGGREGATE_TYPE: &'static str = "User";

    fn aggregate_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn version(&self) -> u64 {
        self.version
    }

    fn apply_event(&mut self, event: &Self::Event) -> Result<()> {
        match event {
            UserEvent::Created(UserCreated { id, name, email }) => {
                self.id = Some(id.clone());
                self.name = name.clone();
                self.email = email.clone();
                self.is_active = false;
                self.version += 1;
            }
            UserEvent::NameChanged(UserNameChanged { name }) => {
                self.name = name.clone();
                self.version += 1;
            }
            UserEvent::EmailChanged(UserEmailChanged { email }) => {
                self.email = email.clone();
                self.version += 1;
            }
            UserEvent::Activated(_) => {
                self.is_active = true;
                self.version += 1;
            }
            UserEvent::Deactivated(_) => {
                self.is_active = false;
                self.version += 1;
            }
        }
        Ok(())
    }
}

//=============================================================================
// Commands
//=============================================================================

/// User commands
#[derive(Debug, Clone)]
enum UserCommand {
    CreateUser {
        id: String,
        name: String,
        email: String,
    },
    ChangeName {
        name: String,
    },
    ChangeEmail {
        email: String,
    },
    Activate,
    Deactivate,
}

impl Command for UserCommand {}

//=============================================================================
// ADR-004: Command Handlers in Aggregates
//=============================================================================

#[async_trait]
impl AggregateRoot for User {
    type Command = UserCommand;

    /// Handle commands with business logic validation
    async fn handle_command(&self, command: Self::Command) -> Result<Vec<Self::Event>> {
        match command {
            // CREATE USER - Validate user doesn't exist
            UserCommand::CreateUser { id, name, email } => {
                if self.id.is_some() {
                    return Err(Error::invalid_command("User already exists"));
                }
                if name.is_empty() {
                    return Err(Error::invalid_command("Name is required"));
                }
                if email.is_empty() || !email.contains('@') {
                    return Err(Error::invalid_command("Valid email is required"));
                }
                Ok(vec![UserCreated { id, name, email }.into()])
            }

            // CHANGE NAME - Validate user exists
            UserCommand::ChangeName { name } => {
                if self.id.is_none() {
                    return Err(Error::invalid_command(
                        "Cannot change name of non-existent user",
                    ));
                }
                if name.is_empty() {
                    return Err(Error::invalid_command("Name cannot be empty"));
                }
                if self.name == name {
                    return Err(Error::invalid_command("Name is already set to this value"));
                }
                Ok(vec![UserNameChanged { name }.into()])
            }

            // CHANGE EMAIL - Validate user exists and email format
            UserCommand::ChangeEmail { email } => {
                if self.id.is_none() {
                    return Err(Error::invalid_command(
                        "Cannot change email of non-existent user",
                    ));
                }
                if email.is_empty() || !email.contains('@') {
                    return Err(Error::invalid_command("Valid email is required"));
                }
                if self.email == email {
                    return Err(Error::invalid_command("Email is already set to this value"));
                }
                Ok(vec![UserEmailChanged { email }.into()])
            }

            // ACTIVATE - Validate user exists and not already active
            UserCommand::Activate => {
                if self.id.is_none() {
                    return Err(Error::invalid_command("Cannot activate non-existent user"));
                }
                if self.is_active {
                    return Err(Error::invalid_command("User is already active"));
                }
                Ok(vec![UserActivated {}.into()])
            }

            // DEACTIVATE - Validate user exists and is active
            UserCommand::Deactivate => {
                if self.id.is_none() {
                    return Err(Error::invalid_command(
                        "Cannot deactivate non-existent user",
                    ));
                }
                if !self.is_active {
                    return Err(Error::invalid_command("User is already inactive"));
                }
                Ok(vec![UserDeactivated {}.into()])
            }
        }
    }
}

#[tokio::main]
async fn main() {
    println!("👤 Basic Aggregate Example - ADR-004 Compliant");
    println!("===============================================\n");

    let mut user = User::default();

    // Step 1: Create User
    println!("📝 Step 1: Create User");
    let create_cmd = UserCommand::CreateUser {
        id: "user-123".to_string(),
        name: "John Doe".to_string(),
        email: "john@example.com".to_string(),
    };
    let events = user.handle_command(create_cmd).await.unwrap();
    for event in &events {
        user.apply_event(event).unwrap();
    }
    println!("   ✓ User created: {}", user.id.as_ref().unwrap());
    println!("   Name: {}", user.name);
    println!("   Email: {}", user.email);

    // Step 2: Activate User
    println!("\n✅ Step 2: Activate User");
    let activate_cmd = UserCommand::Activate;
    let events = user.handle_command(activate_cmd).await.unwrap();
    for event in &events {
        user.apply_event(event).unwrap();
    }
    println!("   ✓ User activated");

    // Step 3: Change Name
    println!("\n📝 Step 3: Change Name");
    let change_name_cmd = UserCommand::ChangeName {
        name: "John Smith".to_string(),
    };
    let events = user.handle_command(change_name_cmd).await.unwrap();
    for event in &events {
        user.apply_event(event).unwrap();
    }
    println!("   ✓ Name changed to: {}", user.name);

    // Step 4: Change Email
    println!("\n📧 Step 4: Change Email");
    let change_email_cmd = UserCommand::ChangeEmail {
        email: "john.smith@example.com".to_string(),
    };
    let events = user.handle_command(change_email_cmd).await.unwrap();
    for event in &events {
        user.apply_event(event).unwrap();
    }
    println!("   ✓ Email changed to: {}", user.email);

    // Final Summary
    println!("\n📊 Final User Summary:");
    println!("   User ID: {}", user.id.as_ref().unwrap());
    println!("   Name: {}", user.name);
    println!("   Email: {}", user.email);
    println!("   Active: {}", user.is_active);

    // Deactivate user
    println!("\n5️⃣ Deactivating User:");
    let deactivate_events = user
        .handle_command(UserCommand::Deactivate)
        .await
        .expect("deactivate");
    for event in deactivate_events {
        user.apply_event(&event).expect("apply deactivate event");
    }
    println!("   Active: {}", user.is_active);

    // Demonstrate Validation
    println!("\n🔒 Demonstrating Business Rule Validation:");
    println!("   Attempting to deactivate already inactive user...");
    let invalid_cmd = UserCommand::Deactivate;
    match user.handle_command(invalid_cmd).await {
        Ok(_) => println!("   ❌ ERROR: Should have been rejected!"),
        Err(e) => println!("   ✓ Correctly rejected: {e:?}"),
    }

    println!("\n✅ ADR-004 Pattern Demonstrated:");
    println!("   • Commands validated in handle_command()");
    println!("   • Events applied in apply_event()");
    println!("   • Business rules enforced");
    println!("   • Invalid operations prevented");
}
