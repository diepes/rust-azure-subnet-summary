# ToDo: Code Review Recommendations

A comprehensive review of the `azure-subnet-summary` Rust application with actionable improvement suggestions organized by priority.

---

## 1. Separation of Concerns (SoC)

### 🔴 High Priority

- [ ] **Extract Azure CLI interaction into a dedicated module**
  - Currently, `graph_read_subnet_data.rs` mixes data parsing, caching logic, and CLI command execution
  - Create `src/azure/` directory with:
    - `mod.rs` - Module exports
    - `cli.rs` - Azure CLI command execution
    - `query.rs` - Graph query building/templates
    - `cache.rs` - Cache read/write operations
  - This allows easier testing and mocking of Azure interactions

- [ ] **Separate data models from business logic**
  - Move `Subnet`, `Ipv4`, `Vnet` structs to `src/models/` directory
  - Keep serialization/deserialization with models
  - Move processing logic (like `process_subnet_row`) to a `src/services/` or `src/processing/` module

- [ ] **Extract output formatting from `subnet_print.rs`**
  - Currently mixes CSV formatting, terminal output, and business logic
  - Create separate:
    - `src/output/csv.rs` - CSV formatting
    - `src/output/terminal.rs` - Terminal/colored output
    - `src/output/mod.rs` - Output trait and common utilities

### 🟡 Medium Priority

- [ ] **Isolate configuration management**
  - `config.rs` only contains a sleep constant - expand to centralize all configuration
  - Add support for:
    - Cache file paths
    - Default CIDR masks
    - Skip patterns for subnets
    - Azure CLI timeout settings
  - Consider using `config` crate or environment variables via `dotenv`

- [ ] **Create a dedicated error module**
  - Replace `Box<dyn std::error::Error>` with custom error types
  - Use `thiserror` crate for ergonomic error definitions
  - Define domain-specific errors: `AzureCliError`, `CacheError`, `ParseError`, `ValidationError`

---

## 2. Code Organization and Modularity ✅ COMPLETED

### 🔴 High Priority

- [x] **Restructure project into logical directories**
  ```
  src/
  ├── main.rs
  ├── lib.rs
  ├── models/
  │   ├── mod.rs
  │   ├── subnet.rs      (from subnet_struct.rs)
  │   ├── ipv4.rs        (from ipv4.rs)
  │   └── vnet.rs        (from struct_vnet.rs)
  ├── azure/
  │   ├── mod.rs
  │   ├── cli.rs         (from cmd.rs)
  │   ├── graph.rs       (query logic from graph_read_subnet_data.rs)
  │   └── cache.rs       (caching from graph_read_subnet_data.rs)
  ├── processing/
  │   ├── mod.rs
  │   ├── dedup.rs       (from de_duplicate_subnets.rs)
  │   └── gap_finder.rs  (from subnet_add_row.rs)
  ├── output/
  │   ├── mod.rs
  │   ├── csv.rs
  │   └── terminal.rs    (from subnet_print.rs)
  ├── config.rs
  └── error.rs
  ```
  **Status:** New module structure created. Legacy modules kept for backwards compatibility with `#[allow(dead_code)]` attributes.

- [x] **Clean up `lib.rs` exports**
  - ~~Currently re-exports functions inconsistently~~
  - ~~Use a clear public API pattern with `pub use` statements~~
  - ~~Hide internal implementation details~~
  **Status:** lib.rs now has clean re-exports from new modules with documentation.

- [x] **Move hardcoded query from `run_az_cli_graph()`**
  - ~~The Azure Graph query string is embedded in the function~~
  - ~~Extract to a constants module or configuration file~~
  **Status:** Query extracted to `SUBNET_QUERY` constant in `azure/graph.rs`.

### 🟡 Medium Priority

- [x] **Consolidate test organization**
  - ~~Move all test data to `tests/` directory (not `src/tests/`)~~
  - ~~Use integration tests in `tests/` for end-to-end scenarios~~
  - ~~Keep unit tests within modules using `#[cfg(test)]`~~
  **Status:** Created `tests/test_data/` and `tests/integration_test.rs`. Legacy tests kept in src/tests for compatibility.

- [x] **Remove dead/commented code**
  - ~~`write_banner.rs` has unused `_write_banner()` function~~ - Deleted
  - ~~`cmd.rs` has commented out `string_to_json_vec_map` and `string_to_json_vec_string`~~ - Removed
  - ~~`lib.rs` has unused `_escape_csv_field`~~ - Removed
  **Status:** Cleaned up dead code. Legacy modules marked with `#[allow(dead_code)]`.

- [x] **Improve naming consistency**
  - ~~`struct_vnet.rs` → `vnet.rs`~~ - New module at `models/vnet.rs`
  - ~~`subnet_struct.rs` → `subnet.rs`~~ - New module at `models/subnet.rs`
  - ~~`de_duplicate_subnets.rs` → `dedup.rs`~~ - New module at `processing/dedup.rs`
  **Status:** New modules use improved naming. Legacy modules kept for backwards compatibility.

### 🟢 Low Priority

- [x] **Add module-level documentation**
  - ~~Add `//!` doc comments at the top of each module explaining its purpose~~
  **Status:** All new modules have `//!` doc comments.
  - Document public APIs with `///` comments
  - Add examples in documentation where helpful

---

## 3. Idiomatic Rust Practices

### 🔴 High Priority

- [ ] **Replace `panic!` with proper error handling**
  - `cmd.rs:36` - panics on command execution error
  - `graph_read_subnet_data.rs:18` - panics if cache file doesn't exist
  - `graph_read_subnet_data.rs:91` - panics on JSON parse error
  - `ipv4.rs:152` - uses `expect()` with string formatting
  - Return `Result` types and propagate errors up

- [ ] **Use custom error types with `thiserror`**
  ```rust
  // Example structure
  #[derive(thiserror::Error, Debug)]
  pub enum AppError {
      #[error("Azure CLI failed: {0}")]
      AzureCli(String),
      #[error("Cache file not found: {0}")]
      CacheNotFound(String),
      #[error("Invalid CIDR format: {0}")]
      InvalidCidr(String),
  }
  ```

- [ ] **Avoid unnecessary clones**
  - `de_duplicate_subnets.rs:31-34` clones `subnet_cidr` and `subscription_id` for sorting
  - `check_for_duplicate_subnets` in `lib.rs` clones for HashSet insertion
  - Consider using references or `Cow<str>` where appropriate

- [ ] **Use `?` operator consistently**
  - Replace `.expect()` calls with `?` where functions return `Result`
  - Example in `graph_read_subnet_data.rs:28`:
    ```rust
    // Before
    serde_json::from_str(&json).expect("Error parsing json")
    // After
    serde_json::from_str(&json)?
    ```

### 🟡 Medium Priority

- [ ] **Improve async usage**
  - `subnet_print()` and `print_vnets()` are async but don't perform async operations
  - Either make them synchronous or add actual async I/O
  - The `#[tokio::main]` could be `#[tokio::main(flavor = "current_thread")]` if parallelism isn't needed

- [ ] **Replace `lazy_static` with `std::sync::OnceLock`** (Rust 1.70+)
  - `cmd.rs` uses `lazy_static` for the regex
  - Modern Rust prefers `OnceLock` or `LazyLock` (Rust 1.80+)

- [ ] **Use `derive_more` or implement `Display` consistently**
  - `Ipv4` has a manual `Display` impl which is good
  - Consider adding `Display` for `Subnet` and `Vnet` for debugging

- [ ] **Replace `extern crate` with `use`**
  - `cmd.rs` uses `extern crate regex` and `extern crate lazy_static`
  - These are no longer needed in Rust 2018+ edition

### 🟢 Low Priority

- [ ] **Use `Iterator` methods more idiomatically**
  - In `subnet_add_row.rs`, the `while` loop building gap subnets could be refactored
  - Consider using `std::iter::successors` or custom iterators

- [ ] **Add `#[must_use]` attributes**
  - Functions that return values that shouldn't be ignored (like validation functions)
  - Example: `check_for_duplicate_subnets` result should be handled

---

## 4. Overall Maintainability

### 🔴 High Priority

- [ ] **Implement a proper application architecture**
  - Consider a layered architecture:
    ```
    ┌─────────────────┐
    │    main.rs      │  Entry point, CLI parsing
    ├─────────────────┤
    │   Application   │  Orchestrates use cases
    ├─────────────────┤
    │    Services     │  Business logic (dedup, gap finding)
    ├─────────────────┤
    │   Repository    │  Data access (Azure CLI, cache)
    ├─────────────────┤
    │     Models      │  Domain objects (Subnet, Vnet, Ipv4)
    └─────────────────┘
    ```

- [ ] **Add CLI argument parsing**
  - Use `clap` crate for command-line argument handling
  - Support options like:
    - `--cache-file <path>` - Specify cache file
    - `--output <format>` - CSV, JSON, or table
    - `--cidr-mask <value>` - Default CIDR mask for gaps
    - `--no-cache` - Force fresh data from Azure
    - `--filter <names>` - Subnet names to ignore

- [ ] **Add integration tests**
  - Create `tests/integration_test.rs`
  - Mock Azure CLI responses for testing
  - Test full workflow with sample data

### 🟡 Medium Priority

- [ ] **Improve logging strategy**
  - Use structured logging with levels appropriately
  - Add log contexts (subscription, vnet names)
  - Consider `tracing` crate for better async support

- [ ] **Add CI/CD configuration**
  - Create `.github/workflows/ci.yml` for GitHub Actions
  - Include: `cargo fmt --check`, `cargo clippy`, `cargo test`
  - Add code coverage with `cargo-tarpaulin` or `cargo-llvm-cov`

- [ ] **Document the architecture**
  - Create `docs/ARCHITECTURE.md` explaining the design
  - Add sequence diagrams for main workflows
  - Document the Azure Graph query structure

### 🟢 Low Priority

- [ ] **Consider workspace organization for larger scope**
  - If adding more Azure tools, use Cargo workspace:
    ```toml
    [workspace]
    members = ["azure-subnet-summary", "azure-common", "azure-cli"]
    ```

- [ ] **Add benchmarks**
  - Use `criterion` crate for performance testing
  - Benchmark IPv4 parsing, subnet calculations
  - Benchmark large dataset processing

- [ ] **Review dependencies**
  - `graph-rs-sdk` - Is this used? Not visible in code
  - `itertools` - Not visible in current code, can remove?
  - `json` crate - Using `serde_json`, might not need both
  - Consider updating `azure_core` and `azure_identity` to latest

---

## 5. Quick Wins (Implement First)

These can be done quickly with high impact:

1. [ ] Run `cargo clippy` and fix all warnings
2. [ ] Run `cargo fmt` for consistent formatting
3. [ ] Remove `extern crate` statements from `cmd.rs`
4. [ ] Remove commented-out code and unused functions
5. [ ] Replace `panic!` in cache file check with `Result` return
6. [ ] Add `.gitignore` entries for generated files if missing
7. [ ] Update `README.md` with usage examples and architecture overview

---

## 6. Dependency Audit

Consider the following dependency changes:

| Current | Recommendation |
|---------|----------------|
| `lazy_static` | Replace with `std::sync::OnceLock` |
| `Box<dyn Error>` | Replace with `thiserror` custom errors |
| No CLI parsing | Add `clap` for argument parsing |
| `log` + `log4rs` | Consider `tracing` for modern async logging |
| `json` crate | Remove if only using `serde_json` |

---

## Progress Tracking

- [ ] Phase 1: Quick Wins (1-2 hours)
- [ ] Phase 2: Error Handling Refactor (2-4 hours)
- [ ] Phase 3: Project Structure Reorganization (4-6 hours)
- [ ] Phase 4: CLI & Configuration (2-3 hours)
- [ ] Phase 5: Testing & Documentation (3-4 hours)

---

*Last updated: 2026-01-22*
