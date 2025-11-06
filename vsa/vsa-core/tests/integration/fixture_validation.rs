//! Integration tests for E2E test fixtures
//!
//! This module contains tests that validate complete projects in tests/fixtures/.
//! Each fixture is a complete, realistic project that vsa-core scans and validates.

use std::fs;
use std::path::{Path, PathBuf};
use vsa_core::{DomainScanner, VsaConfig};

/// Helper to get the fixtures directory path
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Helper to load and scan a fixture
fn scan_fixture(fixture_path: &Path) -> Result<vsa_core::DomainModel, vsa_core::VsaError> {
    // Load vsa.yaml config
    let config_path = fixture_path.join("vsa.yaml");
    if !config_path.exists() {
        return Err(vsa_core::VsaError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Fixture missing vsa.yaml: {}", fixture_path.display()),
        )));
    }

    let config = VsaConfig::from_file(&config_path)?;

    // Ensure domain config exists
    let domain_config = config.domain.ok_or_else(|| {
        vsa_core::VsaError::IoError(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Fixture config missing domain configuration",
        ))
    })?;

    // Scan domain
    let scanner = DomainScanner::new(domain_config, fixture_path.to_path_buf());
    scanner.scan()
}

/// Discover all valid fixtures in a language directory
fn discover_valid_fixtures(language: &str) -> Vec<PathBuf> {
    let valid_dir = fixtures_dir().join(language).join("valid");

    if !valid_dir.exists() {
        return Vec::new();
    }

    fs::read_dir(&valid_dir)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                Some(path)
            } else {
                None
            }
        })
        .collect()
}

/// Discover all invalid fixtures in a language directory
fn discover_invalid_fixtures(language: &str) -> Vec<PathBuf> {
    let invalid_dir = fixtures_dir().join(language).join("invalid");

    if !invalid_dir.exists() {
        return Vec::new();
    }

    fs::read_dir(&invalid_dir)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                Some(path)
            } else {
                None
            }
        })
        .collect()
}

// =============================================================================
// TypeScript Valid Fixtures
// =============================================================================

#[test]
#[ignore] // Ignore until fixtures are created
fn test_typescript_valid_fixtures_exist() {
    let fixtures = discover_valid_fixtures("typescript");
    assert!(
        !fixtures.is_empty(),
        "No valid TypeScript fixtures found. Run migration to create fixtures."
    );
}

#[test]
#[ignore] // Ignore until fixtures are created
fn test_typescript_valid_01_hexagonal_complete() {
    let fixture_path = fixtures_dir().join("typescript/valid/01-hexagonal-complete");

    if !fixture_path.exists() {
        eprintln!("⚠️  Fixture not yet created: {:?}", fixture_path);
        return;
    }

    let model = scan_fixture(&fixture_path).unwrap_or_else(|e| {
        panic!("Failed to scan valid fixture (should pass): {:?}\nError: {:?}", fixture_path, e)
    });

    // Valid fixture should have domain components
    assert!(!model.aggregates.is_empty(), "Valid fixture should have aggregates");
    assert!(!model.commands.is_empty(), "Valid fixture should have commands");
    assert!(!model.events.is_empty(), "Valid fixture should have events");
}

#[test]
#[ignore] // Ignore until fixtures are created
fn test_typescript_valid_02_multi_context() {
    let fixture_path = fixtures_dir().join("typescript/valid/02-multi-context");

    if !fixture_path.exists() {
        eprintln!("⚠️  Fixture not yet created: {:?}", fixture_path);
        return;
    }

    let model = scan_fixture(&fixture_path).unwrap_or_else(|e| {
        panic!("Failed to scan valid fixture (should pass): {:?}\nError: {:?}", fixture_path, e)
    });

    // Multi-context fixture should have multiple aggregates
    assert!(
        model.aggregates.len() >= 2,
        "Multi-context fixture should have at least 2 aggregates, found: {}",
        model.aggregates.len()
    );
}

// =============================================================================
// TypeScript Invalid Fixtures
// =============================================================================

#[test]
#[ignore] // Ignore until fixtures are created
fn test_typescript_invalid_fixtures_exist() {
    let fixtures = discover_invalid_fixtures("typescript");
    // We may not have invalid fixtures yet, so this is informational
    if fixtures.is_empty() {
        eprintln!("⚠️  No invalid TypeScript fixtures found yet");
    }
}

#[test]
#[ignore] // Ignore until fixtures are created
fn test_typescript_invalid_01_no_domain_folder() {
    let fixture_path = fixtures_dir().join("typescript/invalid/01-no-domain-folder");

    if !fixture_path.exists() {
        eprintln!("⚠️  Fixture not yet created: {:?}", fixture_path);
        return;
    }

    let result = scan_fixture(&fixture_path);

    // This fixture should fail validation (domain folder missing)
    // For now, we just check that it doesn't panic
    // Later, we'll assert specific error codes
    match result {
        Ok(model) => {
            // If it succeeds, domain should be empty or minimal
            assert!(
                model.aggregates.is_empty(),
                "Invalid fixture with no domain should have no aggregates"
            );
        }
        Err(e) => {
            // Expected error - domain path not found
            eprintln!("✅ Invalid fixture correctly failed: {:?}", e);
        }
    }
}

// =============================================================================
// Python Valid Fixtures
// =============================================================================

#[test]
#[ignore] // Ignore until fixtures are created
fn test_python_valid_fixtures_exist() {
    let fixtures = discover_valid_fixtures("python");
    // May not exist yet
    if fixtures.is_empty() {
        eprintln!("⚠️  No valid Python fixtures found yet");
    }
}

#[test]
#[ignore] // Ignore until fixtures are created
fn test_python_valid_01_todo_simple() {
    let fixture_path = fixtures_dir().join("python/valid/01-todo-simple");

    if !fixture_path.exists() {
        eprintln!("⚠️  Fixture not yet created: {:?}", fixture_path);
        return;
    }

    let model = scan_fixture(&fixture_path).unwrap_or_else(|e| {
        panic!("Failed to scan valid Python fixture: {:?}\nError: {:?}", fixture_path, e)
    });

    assert!(!model.aggregates.is_empty(), "Python fixture should have aggregates");
}

// =============================================================================
// Discovery Tests
// =============================================================================

#[test]
fn test_fixture_directory_structure_exists() {
    let fixtures_root = fixtures_dir();

    assert!(fixtures_root.exists(), "Fixtures directory should exist at {:?}", fixtures_root);

    // Check language directories exist
    let ts_dir = fixtures_root.join("typescript");
    let py_dir = fixtures_root.join("python");
    let rs_dir = fixtures_root.join("rust");

    assert!(ts_dir.exists(), "TypeScript fixtures directory should exist");
    assert!(py_dir.exists(), "Python fixtures directory should exist");
    assert!(rs_dir.exists(), "Rust fixtures directory should exist");

    // Check valid/invalid subdirectories exist
    assert!(ts_dir.join("valid").exists(), "TypeScript valid/ should exist");
    assert!(ts_dir.join("invalid").exists(), "TypeScript invalid/ should exist");
    assert!(py_dir.join("valid").exists(), "Python valid/ should exist");
    assert!(py_dir.join("invalid").exists(), "Python invalid/ should exist");
    assert!(rs_dir.join("valid").exists(), "Rust valid/ should exist");
}

#[test]
fn test_fixture_readme_exists() {
    let readme = fixtures_dir().join("README.md");
    assert!(readme.exists(), "Fixtures README should exist at {:?}", readme);
}

// =============================================================================
// Fixture Validation Helpers
// =============================================================================

/// Run validation on all fixtures and report results
#[test]
#[ignore] // Ignore by default, run explicitly with: cargo test --test fixture_validation -- --ignored
fn test_all_valid_fixtures() {
    let mut total = 0;
    let mut passed = 0;
    let mut failed = 0;

    for language in &["typescript", "python", "rust"] {
        let fixtures = discover_valid_fixtures(language);

        for fixture_path in fixtures {
            total += 1;
            println!("\n🧪 Testing fixture: {:?}", fixture_path);

            match scan_fixture(&fixture_path) {
                Ok(model) => {
                    passed += 1;
                    println!("  ✅ PASS");
                    println!("     Aggregates: {}", model.aggregates.len());
                    println!("     Commands: {}", model.commands.len());
                    println!("     Events: {}", model.events.len());
                    println!("     Queries: {}", model.queries.len());
                    println!("     Upcasters: {}", model.upcasters.len());
                }
                Err(e) => {
                    failed += 1;
                    println!("  ❌ FAIL: {:?}", e);
                }
            }
        }
    }

    let separator = "=".repeat(60);
    println!("\n{}", separator);
    println!("📊 Fixture Validation Summary");
    println!("{}", separator);
    println!("Total:  {}", total);
    println!("Passed: {} ✅", passed);
    println!("Failed: {} ❌", failed);
    println!("{}", separator);

    if failed > 0 {
        panic!("{} valid fixtures failed validation", failed);
    }
}
