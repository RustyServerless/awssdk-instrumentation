# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-04-27

Re-export every external crate that appears in the public API so that users
no longer have to add the same dependencies to their own `Cargo.toml` simply
to make this crate's macros and traits compile.

### Added

- Crate-root re-exports of `aws_config`, `aws_smithy_runtime_api`,
  `aws_smithy_types`, `opentelemetry`, `opentelemetry_sdk`, and
  `opentelemetry_semantic_conventions`.
- Crate-root re-exports of `tracing`, `tracing_subscriber`, and
  `tracing_opentelemetry`, gated on the `tracing-backend` feature.
- `lambda::LambdaError` and `lambda::LambdaEvent` convenience aliases for
  `lambda_runtime::Error` and `lambda_runtime::LambdaEvent`.
- The `lambda` module now publicly re-exports `lambda_runtime` (previously
  `#[doc(hidden)]`).
- New "Re-exported crates" section in the crate-level documentation.

### Changed

- `aws-config` moved from `dev-dependencies` to `dependencies`, with the
  `behavior-version-latest` feature pinned. The `make_lambda_runtime!` and
  `aws_sdk_config_provider!` macros now reference `aws-config` through the
  crate's re-export, so users no longer need to depend on `aws-config`
  directly.
- `opentelemetry_sdk` is now re-exported at the crate root rather than only
  inside the `lambda` module, matching its use in non-Lambda public APIs.
- Documentation examples updated to use the convenience aliases
  `LambdaError` / `LambdaEvent` and the re-exported `lambda_runtime` path.

### Notes

- Users still need to declare these as direct dependencies: the `aws-sdk-*`
  service crates they use, `tokio` (the `#[tokio::main]` proc-macro emitted
  by `make_lambda_runtime!` resolves `tokio` by absolute path), and
  `serde_json` for typical Lambda event types.

## [0.1.1] - 2026-03-31

(fix) Critical bug in tracing instrumentation crashing the client process.

### Changed

- Reverted an "optimisation" that made wrong assumption about the underlying TracingInterceptor behavior, resulting in systematic failure of AWS SDK Clients.

## [0.1.0] - 2026-03-30 [YANKED]

Initial public release.

### Added

- SDK interceptor (`DefaultInterceptor`) implementing the AWS SDK `Intercept` trait — automatically attaches OpenTelemetry semantic-convention attributes to every AWS SDK call (region, operation, HTTP status, request ID, service-specific attributes).
- `DefaultExtractor` extraction pipeline with per-service dispatch and support for user-registered hooks and custom `AttributeExtractor` implementations.
- `ServiceFilter` for scoping closure hooks to specific services or operations.
- `SpanWrite` trait abstracting span attribute writes across backends.
- `tracing-backend` (default) — writes span attributes via `tracing::Span` + `tracing-opentelemetry`.
- `otel-backend` — manages OTel spans directly without `tracing`.
- Compile-time enforcement that at least one backend is enabled.
- `TracingLayer` Tower layer (`env-lambda`) — creates a per-invocation span, propagates X-Ray trace context, tracks cold-starts, and flushes the exporter after each invocation.
- `make_lambda_runtime!` macro — generates `main()`, telemetry initialisation, SDK client singletons, and Tower layer setup in a single call.
- Lambda resource detector (`env-lambda`).
- `default_resource()` probing Lambda, ECS, EKS, and EC2 environments with fallback to minimal `cloud.provider = aws`.
- ECS resource detector (`env-ecs`) — reads container metadata endpoint.
- EKS resource detector (`env-eks`) — reads Kubernetes service account + IMDSv2.
- EC2 resource detector (`env-ec2`) — reads IMDSv2.
- DynamoDB attribute extractor (`extract-dynamodb`, default) — table name, consumed capacity, etc.
- S3 attribute extractor (`extract-s3`) — bucket name, key, etc.
- SQS attribute extractor (`extract-sqs`) — queue URL, message ID, etc.
- X-Ray ID generator, propagator, and daemon exporter (`export-xray`, default) via `opentelemetry-aws`. Re-exports `opentelemetry_aws` at the crate root.
- `XRAY_ANNOTATIONS` and `XRAY_METADATA` environment variables for controlling X-Ray segment mapping.
- `default_telemetry_init()` — one-call setup: builds a `SdkTracerProvider` with detected resource, X-Ray ID generator, batch exporter, global provider registration, and `tracing-subscriber` with JSON console layer and OTel bridge.
- `default_tracer_provider()` for building the provider without subscriber setup.
- `ParentBased(AlwaysOff)` default sampler on Lambda (respects X-Ray sampling); `ParentBased(AlwaysOn)` outside Lambda.

## [0.0.0] - 2026-02-20

### Added
- Crate.io placeholder

[0.2.0]: https://github.com/RustyServerless/awssdk-instrumentation/releases/tag/v0.2.0
[0.1.1]: https://github.com/RustyServerless/awssdk-instrumentation/releases/tag/v0.1.1
[0.1.0]: https://github.com/RustyServerless/awssdk-instrumentation/releases/tag/v0.1.0
[0.0.0]: https://github.com/RustyServerless/awssdk-instrumentation/releases/tag/v0.0.0
