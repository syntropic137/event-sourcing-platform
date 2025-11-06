//! Integration tests for vsa-core
//!
//! This file serves as the entry point for all integration tests.
//! Each submodule contains tests for specific functionality.

mod integration {
    pub mod fixture_validation;
}

// Re-export for convenience
pub use integration::*;
