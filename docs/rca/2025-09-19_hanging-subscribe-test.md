# Root Cause Analysis: `subscribe_delivers_events` Test Hang

**Date:** 2025-09-19

## 1. Executive Summary

The `subscribe_delivers_events` test in the `eventstore-sdk-rs` crate was hanging indefinitely, causing the CI/CD pipeline to fail. The investigation revealed three distinct root causes: a client-server deadlock, flaky tests due to static port allocation, and a dependency version conflict. All three issues were resolved, and the test suite is now stable and robust.

## 2. Timeline of Events

1.  **Initial Report:** A test named `tests::subscribe_delivers_events` was reported to hang indefinitely during the `make qa` process.
2.  **Initial Investigation:** A timeout was added to the test, which confirmed the hang but did not solve the underlying issue. Debugging with `println!` statements was inconclusive at first.
3.  **Transport Error:** Further test runs revealed a `transport error`, indicating a port conflict. This was traced to a zombie process from a previous failed test run occupying the hardcoded port (`127.0.0.1:60102`).
4.  **Deadlock Identification:** After manually clearing the port, the test proceeded further but still hung during the shutdown sequence (`handle.await`). This pointed to a deadlock where the server was waiting for client connections to close, while the test was waiting for the server to shut down.
5.  **Deadlock Resolution:** The deadlock was resolved by explicitly dropping all client-side gRPC resources (`writer`, `reader`, and especially the `stream`) *before* sending the shutdown signal.
6.  **Robustness Improvement:** To prevent future port conflicts, the hardcoded test ports were replaced with dynamic port allocation using the `portpicker` crate.
7.  **Final Build Error:** With the hanging fixed, a final error emerged during the doc-test phase: `multiple candidates for 'rlib' dependency 'tonic' found`. This was due to a version mismatch between the `[dependencies]` and `[dev-dependencies]` sections in `Cargo.toml`.
8.  **Final Resolution:** The `tonic` dependency was unified to version `0.12` across the board, and the entire test suite passed successfully.

## 3. Root Causes

### Root Cause 1: Client-Server Deadlock in Test Shutdown

-   **Description:** The test initiated a server shutdown while holding an active gRPC streaming connection (`tonic::Streaming`). The `serve_with_shutdown` function in the server gracefully waits for all active connections to terminate before shutting down. Since the test held the stream open while simultaneously awaiting the server's shutdown handle, a classic deadlock occurred.
-   **Trigger:** Improper resource cleanup order in the test's teardown logic.

### Root Cause 2: Flaky Tests Due to Static Port Allocation

-   **Description:** The tests were written to use hardcoded ports (e.g., `60101`, `60102`). If a test failed midway, the server process might not clean up correctly, leaving the port occupied. Subsequent test runs would then fail with a `transport error`, making the test suite flaky and difficult to debug.
-   **Trigger:** Failure of a test to guarantee cleanup of its child server process, combined with the use of a fixed resource (port number).

### Root Cause 3: Dependency Version Conflict

-   **Description:** The `Cargo.toml` file for the `eventstore-sdk-rs` crate specified `tonic = "0.12"` in its main dependencies but `tonic = "0.11.0"` in its dev-dependencies. This caused the Rust compiler to fail during the doc-test phase because it could not decide which version of the library to link.
-   **Trigger:** Lack of version synchronization between dependency sections.

## 4. Corrective and Preventative Actions

1.  **Enforce Strict Resource Cleanup in Tests:**
    -   **Action:** Modified the `subscribe_delivers_events` test to explicitly `drop()` all client-side network resources (clients, streams) before awaiting the server's shutdown handle.
    -   **Prevention:** This pattern should be enforced for all future tests involving client-server interaction. Resources should be cleaned up in the reverse order of their creation.

2.  **Adopt Dynamic Port Allocation for All Network Tests:**
    -   **Action:** Added the `portpicker` crate and updated all network-based tests to request an unused port from the OS for each run (`portpicker::pick_unused_port()`).
    -   **Prevention:** This eliminates port conflicts entirely, making the tests more robust and reliable, especially in parallel or repeated test runs.

3.  **Maintain Strict Dependency Versioning:**
    -   **Action:** Unified the `tonic` dependency to version `0.12` in both `[dependencies]` and `[dev-dependencies]` in the `Cargo.toml` file.
    -   **Prevention:** Regularly audit `Cargo.toml` files for version conflicts. Use tools like `cargo-deny` or similar checks in the CI pipeline to automatically flag duplicate dependencies with mismatched versions.
