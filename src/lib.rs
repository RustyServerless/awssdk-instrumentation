//! Out-of-the-box OpenTelemetry/X-Ray instrumentation for the AWS SDK for Rust,
//! with first-class support for AWS Lambda.
//!
//! This crate wires together three concerns in one place:
//!
//! 1. **SDK interceptors** — automatically attach OTel semantic-convention
//!    attributes to every AWS SDK call (DynamoDB, S3, SQS, …).
//! 2. **Lambda Tower layer** — create a per-invocation span covering the handler,
//!    propagate the X-Ray trace context, track cold-starts, and flush the
//!    exporter after each invocation.
//! 3. **Environment resource detection** — detect whether the process is running
//!    on Lambda, ECS, EKS, or EC2 and populate the OTel [`Resource`] accordingly.
//!
//! The default feature set
//! (`tracing-backend` + `env-lambda` + `extract-dynamodb` + `export-xray`)
//! covers the most common Lambda workload with zero extra configuration.
//!
//! [`Resource`]: opentelemetry_sdk::Resource
//!
//! # Quick Start — Lambda with DynamoDB
//!
//! The fastest path to a fully instrumented Lambda function:
//!
//! ```no_run
//! # mod private {
//! use lambda_runtime::{Error, LambdaEvent};
//! use serde_json::Value;
//!
//! // 1. Declare the handler.
//! async fn handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
//!     // Use dynamodb_client() anywhere in your crate — the interceptor
//!     // automatically records DynamoDB spans.
//!     let resp = dynamodb_client().get_item().table_name("orders").send().await?;
//!     Ok(event.payload)
//! }
//!
//! // 2. One macro call generates main(), telemetry init, and the Tower layer.
//! //    Here you also create a DynamoDB client singleton with the interceptor pre-attached.
//! awssdk_instrumentation::make_lambda_runtime!(
//!     handler,
//!     dynamodb_client() -> aws_sdk_dynamodb::Client
//! );
//! # }
//! # fn dynamodb_client() {}
//! # fn aws_sdk_config() {}
//! # fn main() {}
//! ```
//!
//! # Key Concepts
//!
//! ## Backend
//!
//! Two mutually-exclusive (but co-installable) backends bridge the AWS SDK
//! interceptor and Lambda Tower layer to OpenTelemetry:
//!
//! - **`tracing-backend`** (default) — the interceptor writes attributes into
//!   the active [`tracing::Span`], which is then forwarded to OTel via
//!   `tracing-opentelemetry`. This is the recommended choice: it integrates
//!   naturally with the `tracing` ecosystem and requires no extra setup.
//! - **`otel-backend`** — the interceptor manages OTel spans directly via the
//!   `opentelemetry` API.
//!
//! At least one backend must be enabled; the crate will fail to compile
//! otherwise.
//!
//! The [`interceptor::DefaultInterceptor`] type alias always resolves to the
//! right interceptor for the active backend.
//!
//! ## Interceptors
//!
//! [`interceptor::DefaultInterceptor`] implements the AWS SDK `Intercept` trait.
//! Attach it to any SDK client config to get automatic span attribute extraction:
//!
//! ```no_run
//! # use awssdk_instrumentation::interceptor::DefaultInterceptor;
//! // Attach the interceptor when building the SDK client config.
//! // (aws_config and aws-sdk-dynamodb are not re-exported; add them to your Cargo.toml)
//! # async fn example() {
//! let sdk_config = aws_config::load_from_env().await;
//! let dynamo = aws_sdk_dynamodb::Client::from_conf(
//!     aws_sdk_dynamodb::config::Builder::from(&sdk_config)
//!         .interceptor(DefaultInterceptor::new())
//!         .build(),
//! );
//! # }
//! ```
//!
//! The [`interceptor::DefaultExtractor`] inside the interceptor dispatches to
//! per-service extractors (DynamoDB, S3, SQS) and then runs any user-registered
//! hooks or [`interceptor::AttributeExtractor`] implementations.
//!
//! ## Lambda support
//!
//! The [`lambda`] module (feature `env-lambda`) provides:
//!
//! - [`lambda::layer::TracingLayer`] — a Tower `Layer` that wraps the Lambda
//!   runtime service, creates a span per invocation, and flushes the exporter
//!   when the invocation future drops.
//! - [`make_lambda_runtime!`] — a macro that generates a complete `main()`
//!   function: telemetry init, SDK config/client singletons, Tower layer setup,
//!   and `lambda_runtime::Runtime::run()`.
//!
//! ## Resource detection
//!
//! [`env::default_resource()`] probes the environment at startup and returns an
//! OTel [`Resource`] populated with the appropriate semantic-convention
//! attributes. It tries Lambda first (if feature `env-lambda`),
//! then ECS (if feature `env-ecs`),
//! then EKS (if feature `env-eks`),
//! then EC2 (if feature `env-ec2`),
//! and then falls back to a minimal `cloud.provider = aws` resource.
//!
//! ## Telemetry initialisation
//!
//! [`init::default_telemetry_init()`] is a one-call setup that:
//!
//! - Builds a [`SdkTracerProvider`] with the detected resource, X-Ray ID
//!   generator and a batch X-Ray exporter (when `export-xray` is enabled).
//! - Registers it as the global OTel tracer provider.
//! - Installs a `tracing-subscriber` with a JSON console layer and an OTel
//!   bridge layer (when `tracing-backend` is enabled).
//!
//! [`SdkTracerProvider`]: opentelemetry_sdk::trace::SdkTracerProvider
//!
//! # Manual Setup
//!
//! When you need more control, wire the pieces together yourself:
//!
//! ```no_run
//! use lambda_runtime::{Error, LambdaEvent};
//! use serde_json::Value;
//! use awssdk_instrumentation::{
//!     env::default_resource,
//!     init::{default_telemetry_init, default_tracer_provider},
//!     interceptor::DefaultInterceptor,
//!     lambda::layer::{DefaultTracingLayer, OTelFaasTrigger},
//! };
//!
//! // 1. Declare the handler.
//! async fn handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
//!     todo!("Do Stuff...");
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//! // Initialise telemetry (sets global tracer provider + tracing subscriber).
//! let tracer_provider = default_telemetry_init();
//!
//! // Build an SDK client with the interceptor attached.
//! let sdk_config = aws_config::load_from_env().await;
//! let dynamo = aws_sdk_dynamodb::Client::from_conf(
//!     aws_sdk_dynamodb::config::Builder::from(&sdk_config)
//!         .interceptor(DefaultInterceptor::new())
//!         .build(),
//! );
//!
//! // Wrap the Lambda runtime with the Tower layer.
//! lambda_runtime::Runtime::new(lambda_runtime::service_fn(handler))
//!     .layer(
//!         DefaultTracingLayer::new(move || {
//!             let _ = tracer_provider.force_flush();
//!         })
//!         .with_trigger(OTelFaasTrigger::Http),
//!     )
//!     .run()
//!     .await
//! }
//! ```
//!
//! # Feature Flags
//!
//! Features are grouped by category. Items marked ✅ are enabled by default.
//!
//! ## Backend
//!
//! | Feature           | Default | Description |
//! |-------------------|---------|-------------|
//! | `tracing-backend` | ✅      | Writes span attributes via `tracing::Span` + `tracing-opentelemetry` |
//! | `otel-backend`    |         | Manages OTel spans directly without `tracing` |
//!
//! ## Environment detection
//!
//! | Feature     | Default | Description |
//! |-------------|---------|-------------|
//! | `env-lambda`| ✅      | Lambda Tower layer, resource detector, `make_lambda_runtime!` |
//! | `env-ecs`   |         | ECS resource detector (reads container metadata endpoint) |
//! | `env-eks`   |         | EKS resource detector (reads k8s service account + IMDS) |
//! | `env-ec2`   |         | EC2 resource detector (reads IMDSv2) |
//!
//! ## Service attribute extraction
//!
//! | Feature            | Default | Description |
//! |--------------------|---------|-------------|
//! | `extract-dynamodb` | ✅      | DynamoDB OTel semantic-convention attributes |
//! | `extract-s3`       |         | S3 OTel semantic-convention attributes |
//! | `extract-sqs`      |         | SQS OTel semantic-convention attributes |
//!
//! ## Export
//!
//! | Feature       | Default | Description |
//! |---------------|---------|-------------|
//! | `export-xray` | ✅      | X-Ray ID generator, propagator, and daemon exporter via `opentelemetry-aws` |
//!
//! When `export-xray` is enabled, the `opentelemetry_aws` crate is re-exported
//! at the crate root so you can access the X-Ray propagator and exporter types
//! directly.

// Crate root — re-exports and feature-gated module declarations.

#[cfg(not(any(feature = "otel-backend", feature = "tracing-backend")))]
compile_error!("At least one of \"otel-backend\" or \"tracing-backend\" features must be enabled");

pub mod interceptor;

#[cfg(feature = "env-lambda")]
pub mod lambda;

pub mod env;
pub mod span_write;

pub mod init;

#[cfg(feature = "export-xray")]
pub use opentelemetry_aws;
