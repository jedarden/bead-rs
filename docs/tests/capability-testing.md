# Capability Testing Guide

This guide explains how to run capability detection tests for the bead-rs project, including testing for capability presence and absence across different binary variants.

## Overview

The capability testing framework validates that:

1. The `bead capabilities` command correctly advertises supported features
2. Capabilities match the binary's compiled features (e.g., attempt-resolution)
3. Commands are available/not available based on build configuration
4. Capability advertisements match actual binary behavior

## Test Architecture

### Test Modules

The capability testing framework consists of four main modules:

1. **`tests/capability_framework.rs`** - Core testing utilities
   - `BinaryHarness`: Manages temporary workspaces and binary execution
   - `BinaryVariant`: Describes different build configurations
   - `ExpectedCapabilities`: Defines expected capability state
   - Helper macros: `assert_capability_present!`, `assert_capability_absent!`, etc.

2. **`tests/capability_detection.rs`** - Comprehensive capability tests
   - Tests for default build capabilities
   - Profile-specific tests (native-v1, needle-v1)
   - Schema validation tests
   - Command availability tests

3. **`tests/pinned_binary_capability.rs`** - Binary variant comparison
   - Tests specific to attempt-resolution feature
   - Integration tests for checkpoint round-trips
   - Fallback behavior validation

4. **`tests/binary_variant_integration.rs`** - Binary variant build and comparison
   - Builds multiple binary variants with different features
   - Compares capabilities across variants
   - Validates capability presence/absence
   - Tests command availability across builds

### Key Concepts

**Capabilities Document**: The JSON output from `bead capabilities` that describes:
- Contract version (native-v1, needle-v1)
- Store layout and atomic claim support
- Priority ranges and valid statuses
- Checkpoint modes and formats
- Available commands
- Feature-specific capabilities (auto_flush, attempt_outcome)

**Binary Variants**: Different builds of the binary with different feature sets:
- **Default build**: All features enabled (including attempt-resolution)
- **Minimal build**: Core features only (for comparison)

## Running the Tests

### Quick Start

Run all capability tests with:

```bash
# Run all capability tests (excludes integration tests)
cargo test capability_

# Run with output
cargo test capability_ -- --nocapture

# Run specific test module
cargo test capability_detection
cargo test pinned_binary_capability

# Run specific test
cargo test test_default_build_has_attempt_capability
```

### Running Integration Tests

The binary variant integration tests build multiple binary variants and take longer to run. They are marked as `#[ignore]` and must be explicitly enabled:

```bash
# Run all integration tests (builds multiple binary variants)
cargo test --test binary_variant_integration -- --ignored

# Run specific integration test
cargo test integration_test_default_build_capabilities -- --ignored

# Run with output to see build progress
cargo test --test binary_variant_integration -- --ignored --nocapture
```

**Note**: Integration tests require Cargo in PATH and will compile multiple variants of the binary, which can take several minutes.

### Running Individual Test Suites

```bash
# Test the framework itself
cargo test capability_framework --test capability_framework

# Test capability detection
cargo test capability_detection --test capability_detection

# Test binary variant capabilities
cargo test pinned_binary_capability --test pinned_binary_capability
```

### Running Tests in Parallel vs Serial

Most capability tests use `#[serial]` to prevent conflicts from shared workspace state:

```bash
# Tests run with serial execution automatically when marked
cargo test capability_detection -- --test-threads=1
```

## Test Structure

### Using BinaryHarness

The `BinaryHarness` provides a temporary workspace and executes commands:

```rust
use capability_framework::*;

#[test]
#[serial]
fn my_capability_test() {
    let harness = BinaryHarness::new().unwrap();
    harness.init_workspace().unwrap();

    // Get capabilities
    let caps = harness.get_default_capabilities().unwrap();

    // Test capability presence
    assert!(harness.has_capability_field("attempt_outcome").unwrap());

    // Test command availability
    assert!(harness.command_exists("resolve").unwrap());
}
```

### Defining Expected Capabilities

For comprehensive verification, define expected capabilities:

```rust
let expected = ExpectedCapabilities {
    auto_flush_present: true,
    auto_flush_value: Some(true),
    attempt_outcome_present: true,
    attempt_outcome_supported: true,
    expected_commands: vec!["resolve".to_string()],
    missing_commands: vec!["nonexistent".to_string()],
};

let failures = harness.verify_capabilities(&expected).unwrap();
assert!(failures.is_empty(), "Capability mismatches: {}", failures.join("\n"));
```

### Using Helper Macros

The framework provides assertion macros:

```rust
// Assert capability field exists
assert_capability_present!(harness, "attempt_outcome");
assert_capability_present!(harness, "attempt_outcome.supported");

// Assert capability field is absent
assert_capability_absent!(harness, "nonexistent_field");

// Assert command exists
assert_command_exists!(harness, "resolve");

// Assert command is missing
assert_command_missing!(harness, "nonexistent_command");
```

## What Gets Tested

### Core Capabilities

These are tested for presence/absence:

- **auto_flush**: R026 automatic checkpoint publication (present in default build)
- **attempt_outcome**: ADR-012 attempt resolution (present in default build)
- **Store layout**: Database schema version
- **Atomic claim**: Whether claims use atomic transactions
- **Priority ranges**: Valid priority values (0-4)
- **Statuses**: Valid status values (open, in_progress, closed, deferred)

### Command Availability

Tests verify that commands are available/unavailable:

```rust
// Core commands that must be present
let core_commands = vec![
    "capabilities", "create", "list", "claim", "resolve",
    "close", "reopen", "sync", "why", "doctor",
];

for cmd in core_commands {
    assert_command_exists!(harness, cmd);
}
```

### Profile-Specific Capabilities

Tests verify capabilities for different profiles:

```rust
// Test native-v1 profile
let caps = harness.get_capabilities(&["capabilities", "--profile", "native-v1"])?;
assert_eq!(caps["contract"], "native-v1");

// Test needle-v1 profile
let caps = harness.get_capabilities(&["capabilities", "--profile", "needle-v1"])?;
assert_eq!(caps["contract"], "needle-v1");
```

### Schema Validation

Tests verify schema advertisements:

```rust
// Check schema reference
assert_eq!(caps["schema_ref"], "urn:bead-rs:schema:capabilities:native-v1");

// Verify schemas array
let schemas = caps["schemas"].as_array().unwrap();
assert!(!schemas.is_empty());

// Each schema should have required fields
for schema in schemas {
    assert!(schema.get("schema_ref").is_some());
    assert!(schema.get("document_kind").is_some());
}
```

## Testing Against Different Binary Variants

To test capability absence (e.g., against a build without attempt-resolution):

### Building a Minimal Variant

```bash
# Build without default features
cargo build --no-default-features

# Or build with specific features only
cargo build --no-default-features --features "core"
```

### Testing Against a Custom Binary

```rust
use std::process::Command;

#[test]
#[ignore = "Requires custom-built binary"]
fn test_minimal_build_missing_attempt_capability() {
    let output = Command::new("./target/debug/bead-minimal")
        .args(["capabilities"])
        .current_dir(test_workspace())
        .output()
        .expect("Failed to execute minimal binary");

    let json = String::from_utf8_lossy(&output.stdout);
    // Minimal build should NOT have attempt_outcome
    assert!(!json.contains("attempt_outcome"));
}
```

## Integration with CI/CD

### GitHub Actions Example

```yaml
name: Capability Tests

on: [push, pull_request]

jobs:
  capability-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Run capability tests
        run: cargo test capability_

      - name: Run with specific test
        run: cargo test test_default_build_has_attempt_capability
```

### Argo Workflows Example

```yaml
apiVersion: argoproj.io/v1alpha1
kind: Workflow
metadata:
  name: capability-tests
spec:
  entrypoint: run-tests
  templates:
    - name: run-tests
      steps:
        - - name: checkout
            template: checkout
        - - name: test
            template: capability-tests

    - name: capability-tests
      container:
        image: rust:latest
        command: [sh, -c]
        args: |
          cd /workspace
          cargo test capability_ -- --nocapture
```

## Troubleshooting

### Common Issues

**Tests fail with "workspace already exists"**
- Ensure tests use `#[serial]` annotation
- Use `BinaryHarness::new()` which creates unique temp directories

**Capability detection returns old values**
- Rebuild the binary: `cargo clean && cargo build`
- Check that the correct binary is being tested

**JSON parsing failures**
- Verify `bead capabilities` outputs valid JSON: `bead capabilities | jq .`
- Check for missing required fields in capabilities document

**Command availability tests fail**
- Verify the command exists: `bead resolve --help`
- Check that the command is in the capabilities commands array

### Debugging Failed Tests

```bash
# Run with detailed output
cargo test capability_ -- --nocapture --exact

# Run tests and show output
RUST_BACKTRACE=1 cargo test capability_ -- --nocapture

# Check capabilities manually
cd /tmp && mkdir test-ws && cd test-ws
bead init
bead capabilities | jq .

# Verify specific capability
bead capabilities | jq '.attempt_outcome.supported'
```

### Test Workspace Cleanup

Test workspaces are automatically cleaned up when `BinaryHarness` is dropped. For manual cleanup:

```bash
# Remove test workspaces
rm -rf /tmp/.tmp*

# Or use the harness's cleanup
let harness = BinaryHarness::new()?;
// Test code here...
// harness is automatically dropped when it goes out of scope
```

## Adding New Capability Tests

When adding a new capability to bead-rs:

1. **Add capability to `src/service/capabilities.rs`**
2. **Add test for capability presence in `capability_detection.rs`**:
   ```rust
   #[test]
   #[serial]
   fn new_capability_is_present() {
       let harness = BinaryHarness::new().unwrap();
       assert_capability_present!(harness, "new_capability");
   }
   ```
3. **Add verification to expected capabilities test**
4. **Update this documentation**

## Test Coverage Goals

The capability testing framework aims for:

- ✅ 100% coverage of capability fields
- ✅ 100% coverage of command availability
- ✅ Profile-specific validation (native-v1, needle-v1)
- ✅ Feature flag validation (attempt-resolution, auto_flush)
- ✅ Schema advertisement validation
- ✅ Binary behavior matches capabilities advertisement

## Related Documentation

- [ADR-012: Attempt Resolution](../../docs/boundaries/attempt-resolution-feature.md)
- [Plan Section 11: R026 Handshake](../../docs/plan/plan.md#r026-handshake)
- [Capabilities Service](../../src/service/capabilities.rs)
- [CLI Interface](../../src/cli.rs)

## Quick Reference

```bash
# Run all capability tests
cargo test capability_

# Run specific test module
cargo test capability_detection

# Run with output
cargo test capability_ -- --nocapture

# Check capabilities manually
bead capabilities | jq '.'

# Test specific capability
bead capabilities | jq '.attempt_outcome.supported'

# Verify command exists
bead resolve --help

# Run in serial (for tests that need it)
cargo test pinned_binary_capability -- --test-threads=1
```

For questions or issues with capability testing, see the project documentation or create an issue in the repository.
