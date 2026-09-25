# AGENTS.md

Instructions for AI coding agents operating in this repository.

## Project Overview

Rust library crate providing OpenTelemetry/X-Ray instrumentation for the AWS SDK for Rust,
targeting AWS Lambda workloads. Single-crate project (no workspace), Rust edition 2024.

- **MSRV:** 1.88.0 (declared in `Cargo.toml` `rust-version`)
- **Dev toolchain:** 1.88 (pinned via `rust-toolchain.toml`, a symlink to `nix/rust-toolchain.toml`)
- **License:** MIT

## Build Commands

```sh
# Check compilation
cargo check --all-features

# Lint (zero-warning policy — treats warnings as errors)
cargo clippy --all-targets --all-features -- -D warnings

# Documentation lint (also zero-warning)
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps --document-private-items

# Run all tests
cargo test --all-features

# Run a single test by name
cargo test --all-features <test_name>
# Example: cargo test --all-features it_works

# Run tests in a specific module
cargo test --all-features <module_path>::
```

## Full Validation (run before submitting)

```sh
cargo fmt --check && \
cargo clippy --all-targets --all-features -- -D warnings && \
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps --document-private-items && \
cargo test --all-features
```

## CI Pipeline

CI runs on every push/PR to `main` when `src/`, `Cargo.toml`, `Cargo.lock`, or workflows change.
Three parallel jobs:

| Job  | Toolchain | Commands                                                      |
| ---- | --------- | ------------------------------------------------------------- |
| Lint | stable    | `cargo fmt --check`, clippy, rustdoc (all with `-D warnings`) |
| Test | stable    | `cargo test --all-features`                                   |
| MSRV | 1.88.0    | `cargo check --all-features`, `cargo test --all-features`     |

Clippy runs on **stable** (not MSRV) so the `incompatible_msrv` lint catches APIs newer than
the declared MSRV.

The `.hooks/pre-commit` hook (installed via `./scripts/install-hooks.sh`) runs the exact same
four checks as the Full Validation block. If your commit passes locally, it will pass CI.

## Code Style Guidelines

### General

- Write clean, idiomatic Rust. Rust edition 2024 — use its features but respect MSRV 1.88.0.
- Zero warnings policy: clippy and rustdoc warnings are treated as errors.

### Formatting

- Use `cargo fmt` (default rustfmt settings — no `rustfmt.toml` exists).
- No manual formatting overrides. Let rustfmt handle it.

### Imports

- Group imports logically: std library, external crates, local modules.
- In test modules, use `use super::*;` to import the parent module.
- Prefer specific imports over glob imports (except in test modules).

### Naming Conventions

- Standard Rust naming: `snake_case` for functions, variables, modules; `CamelCase` for types
  and traits; `SCREAMING_SNAKE_CASE` for constants.
- Use descriptive names. Avoid single-letter variables except in closures and iterators.

### Types

- Leverage Rust's type system — prefer newtypes and enums over primitive obsession.
- Use `Result<T, E>` for fallible operations; avoid panicking in library code.
- Prefer `&str` over `String` in function parameters when ownership is not needed.

### Error Handling

- Return `Result` types from fallible functions; avoid panicking in library code.
- `unwrap()`, `expect()`, and `assert!` macros are always acceptable in test code.
- `unwrap()` is acceptable in non-test code **only** when the reason it cannot panic is
  immediately obvious from the surrounding code (e.g., a `None`-check 2 lines earlier).
- `expect("reason")` is acceptable in non-test code when a panic is not possible in
  practice — the message must explain why (e.g., `expect("set in read_before_execution")`).
  Do not use `expect()` as a shortcut to avoid proper error handling.
- Define meaningful error types; consider using `thiserror` for library errors.

### Documentation

- All public APIs must have `///` doc comments — enforced by `cargo doc -D warnings`.
- Include usage examples in doc comments for non-trivial APIs.
- Private items are also checked (`--document-private-items`), so document internal items
  that need explanation.

### Testing

- Unit tests go in a `#[cfg(test)] mod tests` block at the bottom of each source file.
- Integration tests go in the `tests/` directory (when needed).
- Use `assert_eq!`, `assert_ne!`, and `assert!` macros.
- Test names should describe the behavior being tested, using snake_case.
- Always run tests with `--all-features` to ensure full coverage.

### MSRV Discipline

- The MSRV is 1.88.0. Do not use std library APIs or language features introduced after
  this version without updating `rust-version` in `Cargo.toml`.
- Clippy on stable with the `incompatible_msrv` lint will catch violations.
- CI verifies compilation and tests pass on the MSRV toolchain.

## Feature Flags

Features are grouped by category with a prefix convention. Defaults: `tracing-backend`,
`env-lambda`, `extract-dynamodb`, `export-xray`.

| Feature                       | Category    | Gates                                                |
| ----------------------------- | ----------- | ---------------------------------------------------- |
| `tracing-backend`             | Backend     | `tracing` + `tracing-opentelemetry` integration      |
| `otel-backend`                | Backend     | Direct OpenTelemetry span management                 |
| `env-lambda`                  | Environment | Lambda Tower layer, resource detector, macro         |
| `env-ecs`                     | Environment | ECS resource detector                                |
| `env-eks`                     | Environment | EKS resource detector                                |
| `env-ec2`                     | Environment | EC2 resource detector                                |
| `extract-dynamodb`            | Extraction  | `aws-sdk-dynamodb` dep, DynamoDB attribute extractor |
| `extract-s3`                  | Extraction  | `aws-sdk-s3` dep, S3 attribute extractor             |
| `extract-sqs`                 | Extraction  | `aws-sdk-sqs` dep, SQS attribute extractor           |
| `export-xray`                 | Export      | `opentelemetry-aws` dep, X-Ray ID generator/exporter |
| `xray-no-lambda-node-nesting` | Export      | Opt-in span-kind tweak for X-Ray Lambda node nesting |

At least one backend feature must be enabled (enforced by `compile_error!` in `lib.rs:260`).

Note: `tracing` and `tracing-subscriber` are **always** compiled in (non-optional deps), even
under `otel-backend` — the SDK's `Service.Operation` is only reachable via a `tracing` span the
SDK emits. Do not assume the tracing stack is absent when only `otel-backend` is enabled.

## Project Structure

```
src/
├── lib.rs                          # Re-exports, feature gates, crate-level docs
├── init.rs                         # default_telemetry_init / default_tracer_provider
│                                   #   builders; aws_sdk_config_provider! &
│                                   #   aws_sdk_client_provider! macros. Resource
│                                   #   detection delegates to opentelemetry_aws::detector
│                                   #   (gated by env-* features); no local detector code.
├── span_write/
│   ├── mod.rs                      # SpanWrite trait definition
│   ├── tracing.rs                  # SpanWrite impl for tracing::Span (tracing-backend)
│   └── otel.rs                     # SpanWrite impl for OTel Span (otel-backend)
├── interceptor/
│   ├── mod.rs                      # AttributeExtractor trait, DefaultExtractor,
│   │                               #   ServiceFilter, closure registration, dispatch
│   ├── utils.rs                    # Internal helpers (operation parsing, span pausing)
│   ├── tracing.rs                  # TracingInterceptor (tracing-backend)
│   ├── otel.rs                     # OtelInterceptor (otel-backend)
│   └── extract/
│       ├── mod.rs                  # Metadata extraction, service dispatch
│       ├── dynamodb.rs             # (extract-dynamodb)
│       ├── s3.rs                   # (extract-s3)
│       └── sqs.rs                  # (extract-sqs)
└── lambda/                         # (env-lambda)
    ├── mod.rs                      # Lambda module re-exports (OTelFaasTrigger, lambda_runtime)
    ├── macros.rs                   # make_lambda_runtime! macro
    └── layer/
        ├── mod.rs                  # Shared layer machinery: TracingLayer, TracingService,
        │                           #   Instrumentor/InstrumentedFuture traits, InvocationContext
        ├── utils.rs                # OTelFaasTrigger enum, layer helpers
        ├── tracing.rs              # TracingInstrumentor (Instrumentor impl, tracing-backend)
        └── otel.rs                 # OtelInstrumentor (Instrumentor impl, otel-backend)
```

## Release Process

Maintainers only. Bump version in `Cargo.toml`, update `CHANGELOG.md`, tag with `vX.Y.Z`.
The `release.yml` workflow runs lint+test+MSRV checks then publishes to crates.io.
