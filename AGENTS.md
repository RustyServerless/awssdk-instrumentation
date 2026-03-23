# AGENTS.md

Instructions for AI coding agents operating in this repository.

## Project Overview

Rust library crate providing OpenTelemetry/X-Ray instrumentation for the AWS SDK for Rust,
targeting AWS Lambda workloads. Single-crate project (no workspace), Rust edition 2024.

- **MSRV:** 1.85.0 (declared in `Cargo.toml` `rust-version`)
- **Dev toolchain:** 1.93 (pinned via `rust-toolchain.toml`)
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

## CI Pipeline

CI runs on every push/PR to `main` when `src/`, `Cargo.toml`, `Cargo.lock`, or workflows change.
Three parallel jobs:

| Job | Toolchain | Commands |
|------|-----------|----------|
| Lint | stable | `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps --document-private-items` |
| Test | stable | `cargo test --all-features` |
| MSRV | 1.85.0 | `cargo check --all-features`, `cargo test --all-features` |

Clippy runs on **stable** (not MSRV) so the `incompatible_msrv` lint catches APIs newer than
the declared MSRV. If your commit passes locally, it will pass CI.

## Full Validation (run before submitting)

```sh
cargo fmt --check && \
cargo clippy --all-targets --all-features -- -D warnings && \
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps --document-private-items && \
cargo test --all-features
```

## Code Style Guidelines

### General

- Write clean, idiomatic Rust.
- Rust edition 2024 — use its features (e.g., `gen` blocks, `use<>` bounds) but respect MSRV 1.85.0.
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

- Return `Result` types from fallible functions; do not `unwrap()` or `expect()` in
  non-test code.
- `unwrap()`, `expect()`, and `assert!` macros are acceptable in test code.
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
- Run tests with `--all-features` to ensure full coverage.

### MSRV Discipline

- The MSRV is 1.85.0. Do not use std library APIs or language features introduced after
  this version without updating `rust-version` in `Cargo.toml`.
- Clippy on stable with the `incompatible_msrv` lint will catch violations.
- CI verifies compilation and tests pass on the MSRV toolchain.

## Architecture & Design

### Goal

Allow users of the AWS SDK for Rust to produce OpenTelemetry spans with service-specific
attributes (e.g. DynamoDB table name, S3 bucket, SQS queue URL) for every AWS service call,
with minimal code changes. Primary target is AWS Lambda + X-Ray, but the crate also supports
ECS/EKS/EC2 environments and arbitrary OpenTelemetry exporters.

### Core Traits

#### `SpanWrite`

Abstraction over writing attributes and status to a span, regardless of the underlying
tracing/telemetry library. One implementation per backend:

- **tracing backend** (`tracing-backend` feature): implemented on `tracing::Span`, injects attributes
  via span extensions.
- **OTel-native backend** (`otel-backend` feature): implemented on `opentelemetry_sdk::trace::Span`,
  sets attributes and status directly.

Uses `opentelemetry::Value` as the attribute value type to avoid an intermediate representation.

```rust
trait SpanWrite {
    fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>);
    fn set_status(&mut self, code: StatusCode, message: impl Into<String>);
    // May grow: set_name, set_span_kind, etc.
}
```

#### `AttributeExtractor<SW: SpanWrite>`

Defines extraction logic that reads from AWS SDK context objects (Input, Output, Error) and
writes attributes into a `&mut SW`. The crate provides a `DefaultExtractor<SW>` that:

1. Dispatches by service name (from `Metadata` in the `ConfigBag`) and operation name.
2. Downcasts the type-erased `Input`/`Output` to concrete SDK types (e.g.
   `aws_sdk_dynamodb::operation::put_item::PutItemInput`) using feature-gated dependencies.
3. Extracts relevant attributes and writes them via `SpanWrite`.

Users can extend extraction in two ways:

- **Closure registration:** register `Fn(&mut SW, &str, &str, &Input)` closures scoped by a
  `ServiceFilter` (all services, specific service, specific service+operation). Quick one-off
  additions.
- **Trait implementation:** implement `AttributeExtractor<SW>` for structured multi-operation
  extraction logic.

User-registered extractors and closures always run **after** the built-in extraction, allowing
overrides.

### Interceptor Model

The crate implements the `aws_smithy_runtime_api::client::interceptors::Intercept` trait.
There is **one concrete interceptor type per backend** (e.g. `TracingInterceptor`,
`OtelInterceptor`), each parameterized internally over the extraction pipeline.

The interceptor is generic across all AWS services — service detection uses `Metadata` from
the `ConfigBag` (available at every hook via `cfg.load::<Metadata>()`), which provides
`service()` and `name()` (operation) with no per-service dependency.

**Hook usage:**

| Hook                    | Purpose                                                  |
|-------------------------|----------------------------------------------------------|
| `read_before_execution` | Start span (or retrieve existing tracing span), extract  |
|                         | service/operation from `Metadata`, extract attributes    |
|                         | from `Input` via downcast.                               |
| `read_after_execution`  | Extract attributes from `Output` or error information    |
|                         | from `OrchestratorError<Error>`, set span status, end    |
|                         | span.                                                    |

Additional hooks may be used in the future (e.g. `read_after_serialization` for HTTP-level
attribute extraction as a fallback).

### Instrumentation Backends

#### tracing backend (feature: `tracing-backend`, **default**)

Retrieves the `tracing::Span` that the AWS SDK already creates internally for each service
call. Injects additional attributes into it. Relies on `tracing-opentelemetry` to translate
tracing spans into OpenTelemetry spans with correct parent-child relationships.

**Advantages:** integrates naturally if the user already uses `tracing`; the span boundaries
match the SDK's own timing (more accurate than interceptor hooks).

**Limitations:** `tracing-opentelemetry` cannot change span kind after creation; some OTel
attributes require workarounds.

#### OTel-native backend (feature: `otel-backend`)

Creates and manages an `opentelemetry::trace::Span` directly in the interceptor hooks. Full
control over all span attributes, kind, and status.

**Advantages:** no dependency on `tracing`; full OTel compliance.

**Limitations:** span boundaries are slightly narrower than reality (hooks fire after/before
the SDK's own start/end).

### Lambda Support (feature: `env-lambda`)

#### Tower Layer

A `lambda_runtime::tower::Layer` that creates an invocation span per Lambda execution. This
span is the parent of all AWS SDK call spans within that invocation. Handles:

- Extracting `_X_AMZN_TRACE_ID` and setting it as parent context.
- Setting invocation-specific attributes (function name, request ID, cold start, etc.).
- Flushing the span exporter after each invocation (critical to avoid span loss on freeze).

The layer's backend (tracing vs OTel-native) aligns with the interceptor backend choice.

#### `make_lambda_runtime!` Macro

A macro-rules macro for the mainstream happy path. Generates `main()`, initializes the
tracer, creates instrumented SDK clients, sets up the Tower layer, and runs the Lambda
runtime.

```rust
make_lambda_runtime!(my_handler, dynamodb() -> aws_sdk_dynamodb::Client, s3() -> aws_sdk_s3::Client)
```

The generated code uses helper functions internally. Users with advanced needs (custom
`SdkConfig`, custom endpoints, etc.) bypass the macro and use the helpers directly.

### Environment Resource Detection

Each environment feature gates a `ResourceDetector` implementation that populates the
OpenTelemetry `Resource` with environment-specific attributes:

| Feature      | Environment | Attributes                                     |
|--------------|-------------|------------------------------------------------|
| `env-lambda` | AWS Lambda  | function name, version, memory, log group, etc.|
| `env-ecs`    | Amazon ECS  | cluster, task ARN, container ID, etc.          |
| `env-eks`    | Amazon EKS  | cluster name, pod, namespace, etc.             |
| `env-ec2`    | Amazon EC2  | instance ID, AMI, availability zone, etc.      |

### Tracer Initialization

The crate provides a builder for `TracerProvider` setup with sensible defaults:

- Installs the appropriate resource detectors based on enabled features.
- Configures the X-Ray propagator and ID generator when `export-xray` is enabled.
- Configures `tracing-opentelemetry` subscriber layer when `tracing-backend` is enabled.
- Allows user overrides for span processor, exporter, resource, and other settings.

For non-Lambda environments, the user is responsible for calling the builder in their own
initialization code. For Lambda, the macro or layer handles it.

### Feature Flags

Features are grouped by category with a prefix convention:

| Feature             | Category    | Gates                                              | Default |
|---------------------|-------------|----------------------------------------------------|---------|
| `tracing-backend`   | Backend     | `tracing` + `tracing-opentelemetry` deps,          | **yes** |
|                     |             | `TracingInterceptor`, `TracingSpanWriter`           |         |
| `otel-backend`      | Backend     | `OtelInterceptor`, `OtelSpanWriter`                | no      |
| `env-lambda`        | Environment | Lambda Tower layer, resource detector, macro,      | **yes** |
|                     |             | `lambda_runtime` + `aws_lambda_events` deps        |         |
| `env-ecs`           | Environment | ECS resource detector                              | no      |
| `env-eks`           | Environment | EKS resource detector                              | no      |
| `env-ec2`           | Environment | EC2 resource detector                              | no      |
| `extract-dynamodb`  | Extraction  | `aws-sdk-dynamodb` dep, DynamoDB extraction module | **yes** |
| `extract-s3`        | Extraction  | `aws-sdk-s3` dep, S3 extraction module             | no      |
| `extract-sqs`       | Extraction  | `aws-sdk-sqs` dep, SQS extraction module           | no      |
| `export-xray`       | Export      | `opentelemetry-aws` dep, X-Ray propagator,         | **yes** |
|                     |             | ID generator, exporter config                      |         |

**Default features:** `tracing-backend`, `env-lambda`, `extract-dynamodb`, `export-xray` —
targeting the primary audience of Lambda + tracing + DynamoDB + X-Ray users.

None of the defaults are mandatory. Users can disable any default and enable alternatives
(e.g. `default-features = false, features = ["otel-backend", "extract-s3"]`).

When multiple backend features are enabled (e.g. due to feature unification from transitive
dependencies), the user must explicitly select which backend to use via configuration — the
crate does not silently pick one.

## Project Structure

```
src/
├── lib.rs                          # Re-exports, feature gates, crate-level docs
├── span_write.rs                   # SpanWrite trait definition
├── interceptor/
│   ├── mod.rs                      # AttributeExtractor trait, DefaultExtractor,
│   │                               #   ServiceFilter, closure registration, dispatch
│   ├── tracing.rs                  # TracingSpanWriter + TracingInterceptor
│   │                               #   (feature: tracing-backend)
│   ├── otel.rs                     # OtelSpanWriter + OtelInterceptor
│   │                               #   (feature: otel-backend)
│   └── extract/
│       ├── mod.rs                  # Metadata extraction (always), service dispatch
│       ├── dynamodb.rs             # (feature: extract-dynamodb)
│       ├── s3.rs                   # (feature: extract-s3)
│       └── sqs.rs                  # (feature: extract-sqs)
├── lambda/                         # (feature: env-lambda)
│   ├── mod.rs                      # Lambda module re-exports
│   ├── layer.rs                    # Tower Layer for invocation spans
│   └── macros.rs                   # make_lambda_runtime! macro
├── env/
│   ├── mod.rs                      # Common resource detection types
│   ├── lambda.rs                   # (feature: env-lambda)
│   ├── ecs.rs                      # (feature: env-ecs)
│   ├── eks.rs                      # (feature: env-eks)
│   └── ec2.rs                      # (feature: env-ec2)
└── init.rs                         # TracerProvider builder, setup helpers

.hooks/pre-commit                   # Git pre-commit hook
.github/workflows/                  # CI (ci.yml) and release (release.yml) pipelines
nix/                                # Nix flake dev environment
scripts/                            # Helper scripts (install-hooks.sh)
```

## Release Process

Maintainers only. Bump version in `Cargo.toml`, update `CHANGELOG.md`, tag with `vX.Y.Z`.
The `release.yml` workflow runs lint+test+MSRV checks then publishes to crates.io.
