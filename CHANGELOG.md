# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.1]: https://github.com/RustyServerless/awssdk-instrumentation/releases/tag/v0.1.1
[0.1.0]: https://github.com/RustyServerless/awssdk-instrumentation/releases/tag/v0.1.0
[0.0.0]: https://github.com/RustyServerless/awssdk-instrumentation/releases/tag/v0.0.0
