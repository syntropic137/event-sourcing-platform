//! WASM bindings for VSA Core
//!
//! This module provides WebAssembly bindings for Node.js/browser usage.

use wasm_bindgen::prelude::*;

// Set panic hook for better error messages
#[wasm_bindgen(start)]
pub fn main() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
#[derive(Default)]
pub struct VsaValidator {
    // TODO: Add validation state
}

#[wasm_bindgen]
impl VsaValidator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn validate(&self, _config_json: &str) -> Result<String, JsValue> {
        // TODO: Implement WASM validation
        Ok("{}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    fn test_validator_creation() {
        let validator = VsaValidator::new();
        assert!(validator.validate("{}").is_ok());
    }
}
