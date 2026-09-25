<!-- PROJECT SHIELDS -->

[![crates.io](https://img.shields.io/crates/v/awssdk-instrumentation.svg)](https://crates.io/crates/awssdk-instrumentation)
[![docs.rs](https://docs.rs/awssdk-instrumentation/badge.svg)](https://docs.rs/awssdk-instrumentation/latest/awssdk_instrumentation)
[![CI](https://github.com/RustyServerless/awssdk-instrumentation/workflows/CI/badge.svg)](https://github.com/RustyServerless/awssdk-instrumentation/actions)
[![License](https://img.shields.io/github/license/RustyServerless/awssdk-instrumentation.svg)](https://github.com/RustyServerless/awssdk-instrumentation/blob/main/LICENSE)

# awssdk-instrumentation

Out-of-the-box OpenTelemetry/X-Ray instrumentation for the AWS SDK for Rust, with first-class support for AWS Lambda.

<details>
  <summary>Table of Contents</summary>
  <ol>
    <li><a href="#about-the-project">About The Project</a></li>
    <li><a href="#features">Features</a></li>
    <li><a href="#getting-started">Getting Started</a></li>
    <li><a href="#usage">Usage</a></li>
    <li><a href="#feature-flags">Feature Flags</a></li>
    <li><a href="#configuration">Configuration</a></li>
    <li><a href="#minimum-supported-rust-version-msrv">Minimum Supported Rust Version</a></li>
    <li><a href="#faq">FAQ</a></li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
    <li><a href="#authors">Authors</a></li>
  </ol>
</details>

## About The Project

`awssdk-instrumentation` wires together three concerns that every instrumented AWS workload needs:

1. **SDK interceptors** — automatically attach OpenTelemetry semantic-convention attributes to every AWS SDK call (DynamoDB, S3, SQS, and more via user-defined extractors).
2. **Lambda Tower layer** — create a per-invocation span covering the handler, propagate the X-Ray trace context, track cold-starts, and flush the exporter after each invocation.
3. **Environment resource detection** — the `opentelemetry-aws` detectors enabled by your `env-*` features populate the OTel `Resource` with Lambda, ECS, EKS, and EC2 attributes.

The default feature set (`tracing-backend` + `env-lambda` + `extract-dynamodb` + `export-xray`) covers the most common Lambda workload with zero extra configuration.

## Features

- Automatic OTel span enrichment for every AWS SDK call (region, operation, HTTP status, request ID, service-specific attributes)
- Per-invocation Lambda spans with X-Ray trace context propagation and cold-start tracking
- Built-in attribute extractors for DynamoDB, S3, and SQS
- Extensible extraction pipeline: register custom `AttributeExtractor` implementations or closure hooks filtered by service/operation
- AWS environment attributes (Lambda, ECS, EKS, EC2) added to the OTel `Resource` by the `opentelemetry-aws` detectors behind each `env-*` feature
- X-Ray ID generation and daemon export out of the box
- `make_lambda_runtime!` macro for zero-boilerplate Lambda setup
- Two backend options: `tracing` ecosystem integration (default) or direct OTel span management

## Getting Started

### Prerequisites

- Rust 1.88.0 or later
- An AWS SDK for Rust client (`aws-sdk-dynamodb`, `aws-sdk-s3`, etc.)
- For Lambda workloads: `tokio`.

### Installation

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
awssdk-instrumentation = "0.3"
```

Or using cargo:

```sh
cargo add awssdk-instrumentation
```

The default features (`tracing-backend`, `env-lambda`, `extract-dynamodb`, `export-xray`) are suitable for most Lambda + DynamoDB workloads. Disable the default features and see [Feature Flags](#feature-flags) to customise.

### Try on AWS 🚀

Want to see the crate in action? [Yak Mania](https://github.com/RustyServerless/yak-mania) is a ready-to-deploy playground on GitHub that sets up a full AWS environment in a few minutes, with two ways in:

- 🎮 A fun little game built on AppSync + Lambda + DynamoDB, generating traces with annotations and metadata — real application tracking on AWS X-Ray, end to end.
- 📊 A comparative benchmark of Python and Rust on a simple DynamoDB interaction:
  - Python without instrumentation,
  - Python with X-Ray (old-school) instrumentation,
  - Python with ADOT v1 instrumentation,
  - Python with ADOT v2 instrumentation,
  - Rust without instrumentation,
  - Rust with `awssdk-instrumentation`, `tracing-backend`,
  - Rust with `awssdk-instrumentation`, `otel-backend`.

## Usage

> **Re-exports.** To minimise the dependencies you need to declare in your own `Cargo.toml`, this crate re-exports every external crate that appears in its
> public API: `aws-config`, `aws-smithy-runtime-api`, `aws-smithy-types`, `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-semantic-conventions`,
> plus `tracing` / `tracing-subscriber` / `tracing-opentelemetry` (under `tracing-backend`), `lambda_runtime` (under `env-lambda`), and
> `opentelemetry-aws` (when any `env-*` or `export-xray` feature is enabled). All are available via `awssdk_instrumentation::<crate>`, except `lambda_runtime`, which is
> re-exported at `awssdk_instrumentation::lambda::lambda_runtime`.
>
> You still need to add the following to your own `Cargo.toml`:
>
> - The `aws-sdk-*` service crates you use (`aws-sdk-dynamodb`, `aws-sdk-s3`, …).
> - `tokio` — the `#[tokio::main]` proc-macro emitted by `make_lambda_runtime!` resolves the `tokio` crate by absolute path (`::tokio`), so re-exporting
>   would not help. Lambda functions in Rust need `tokio` anyway.
> - `serde_json` (or `serde`) — for typical Lambda event types.

### Quick Start — Lambda with DynamoDB

The `make_lambda_runtime!` macro generates `main()`, telemetry initialisation, SDK client singletons, and the Tower layer in a single call:

```rust
use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent};
use serde_json::Value;

// 1. Declare the handler.
async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    // Use dynamodb_client() anywhere — the interceptor
    // automatically records DynamoDB spans.
    let _resp = dynamodb_client()
        .get_item()
        .table_name("orders")
        .send()
        .await?;
    Ok(event.payload)
}

// 2. One macro call generates main(), telemetry init, and the Tower layer.
//    Client declarations produce OnceLock-backed singletons with the
//    interceptor pre-attached.
awssdk_instrumentation::make_lambda_runtime!(
    handler,
    dynamodb_client() -> aws_sdk_dynamodb::Client
);
```

The macro also accepts optional named parameters: `trigger` (the `faas.trigger` value), `telemetry_init` (a custom init function), `extra_init_code` (code to run just before the runtime starts), `sdk_client_interceptor` (an explicit interceptor for the generated clients), and `lambda_layer_instrumentor` (an explicit Tower layer instrumentor).

### Manual Setup

When you need more control over the telemetry stack, wire the pieces together yourself. As a starting point,
here is the exact code generated by the expansion of the previous snippet:

```rust
use awssdk_instrumentation::{
    init::default_telemetry_init,
    interceptor::DefaultInterceptor,
    lambda::{
        LambdaError, LambdaEvent, lambda_runtime,
        layer::{DefaultInstrumentor, OTelFaasTrigger, TracingLayer},
        macros::default_flush_tracer,
    },
};
use serde_json::Value;

// Same Handler
async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    let _resp = dynamodb_client()
        .get_item()
        .table_name("orders")
        .send()
        .await?;
    Ok(event.payload)
}

// This macro sets a OnceCell + Accessor function containing the AWS Credentials Configuration
// made available and shared by the SDK Clients.
// It generates the sdk_config_init() function, which should be called once, early in your main.
awssdk_instrumentation::aws_sdk_config_provider!();

// The macro sets a OnceCell + Accessor function for the
// DynamoDB client.
// It uses an "Interceptor", depending on the backend you chose (i.e. `tracing` or `otel`).
// The Interceptor's role is to hook into the SDK Client interactions to extract informations
// and add them to the spans (e.g. DynamoDB table name).
// Generates the dynamodb_client() function that returns a AWS SDK DynamoDB client.
awssdk_instrumentation::aws_sdk_client_provider!(
    dynamodb_client() -> aws_sdk_dynamodb::Client,
    interceptor = DefaultInterceptor::new()
);

#[tokio::main]
async fn main() -> Result<(), LambdaError> {
    // This is the default telemetry initialization, can be controlled by the
    // `telemetry_init` parameter of the macro.
    // Any function returning the SdkTracerProvider that was made "global"
    // can be used.
    //
    // What the default_telemetry_init() function foes depends on your features.
    // With `export-xray` and `tracing-backend`, it setups the X-Ray export,
    // the tracing=>otel translation layer and a console logger that will send
    // your outputs to stdout.
    let tracer_provider = default_telemetry_init();

    // Created by the aws_sdk_config_provider! macro, retrieve
    // AWS credentials from the environment, as usual.
    sdk_config_init().await;

    lambda_runtime::Runtime::new(lambda_runtime::service_fn(handler))
        // This adds the "tracing" Tower Layer for Lambda
        // It uses an "Instrumentor", depending on the backend you chose (i.e. `tracing` or `otel`)
        .layer(
            <TracingLayer<_, DefaultInstrumentor>>::new(move || {
                // This ensures that when your handler finishes,
                // the traces are flushed to the exporter, i.e. to
                // the X-Ray daemon.
                default_flush_tracer(&tracer_provider);
            })
            // This tells the tracing layer which "Trigger" it should set
            // on the span that will cover your execution.
            // Defaults to Http.
            .with_trigger(OTelFaasTrigger::default()),
        )
        .run()
        .await
}
```

You can of course wire things yourself further, without the provided macros and functions. Read the expanded code.

### Extending the Extraction Pipeline

For simple, scoped customisations — targeting a single service or operation — closure hooks are the most convenient approach:

```rust
use awssdk_instrumentation::interceptor::{DefaultInterceptor, ServiceFilter};
use awssdk_instrumentation::span_write::SpanWrite;

let mut interceptor = DefaultInterceptor::new();

// Add a custom attribute to every DynamoDB GetItem call.
interceptor.extractor.register_input_hook(
    ServiceFilter::Operation("DynamoDB", "GetItem"),
    |_service, _operation, _input, span| {
        span.set_attribute("app.table", "orders");
    },
);
```

For more complex extraction logic — spanning multiple phases or services — implement the `AttributeExtractor` trait instead:

```rust
use awssdk_instrumentation::interceptor::{
    context,
    AttributeExtractor, DefaultInterceptor, Operation, Service,
};
use awssdk_instrumentation::span_write::SpanWrite;

struct OrdersExtractor;

impl<SW: SpanWrite> AttributeExtractor<SW> for OrdersExtractor {
    fn extract_input(
        &self,
        service: Service,
        operation: Operation,
        _input: &context::Input,
        span: &mut SW,
    ) {
        if service == "DynamoDB" && operation == "GetItem" {
            span.set_attribute("app.table", "orders");
        }
    }
}

let mut interceptor = DefaultInterceptor::new();
interceptor.extractor.register_attribute_extractor(OrdersExtractor);
```

> **Contributions welcome:** additional service extractors and extraction logic improvements are very welcome and likely to be merged quickly. See [Contributing](#contributing).

## Feature Flags

Features are grouped by category. Items marked **✅** are enabled by default.

### Backend

At least one backend must be enabled (enforced at compile time).

| Feature           | Default | Description                                                                                                              |
| ----------------- | ------- | ------------------------------------------------------------------------------------------------------------------------ |
| `tracing-backend` | ✅      | Writes span attributes via `tracing::Span` + `tracing-opentelemetry`. Integrates naturally with the `tracing` ecosystem. |
| `otel-backend`    |         | Manages OTel spans directly, so your application can emit spans and events with `opentelemetry` primitives only          |

### Environment Detection

| Feature      | Default | Description                                                                                     |
| ------------ | ------- | ----------------------------------------------------------------------------------------------- |
| `env-lambda` | ✅      | Lambda Tower layer, `opentelemetry-aws` Lambda resource detector, `make_lambda_runtime!` macro. |
| `env-ecs`    |         | Enables the `opentelemetry-aws` `EcsResourceDetector`.                                          |
| `env-eks`    |         | Enables the `opentelemetry-aws` `EksResourceDetector`.                                          |
| `env-ec2`    |         | Enables the `opentelemetry-aws` `Ec2ResourceDetector`.                                          |

### Service Attribute Extraction

| Feature            | Default | Description                                                                         |
| ------------------ | ------- | ----------------------------------------------------------------------------------- |
| `extract-dynamodb` | ✅      | DynamoDB OTel semantic-convention attributes (table name, consumed capacity, etc.). |
| `extract-s3`       |         | S3 OTel semantic-convention attributes (bucket name, key, etc.).                    |
| `extract-sqs`      |         | SQS OTel semantic-convention attributes (queue URL, message ID, etc.).              |

### Export

| Feature                       | Default | Description                                                                                                                                                       |
| ----------------------------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `export-xray`                 | ✅      | X-Ray ID generator and daemon exporter via `opentelemetry-aws`.                                                                                                   |
| `xray-no-lambda-node-nesting` |         | Emit the Lambda invocation span as `server` kind so its X-Ray segment appears as a separate `AWS::Lambda::Function` node instead of nested under the service one. |

When a feature that depends on `opentelemetry-aws` is enabled, the `opentelemetry_aws` crate is re-exported at the crate root, making the X-Ray propagator available for usage.

## Configuration

### X-Ray Annotations and Metadata

When `export-xray` is enabled, every span attribute is exported as metadata by default.

Additionaly, two environment variables control how span attributes are mapped to X-Ray segments:

| Variable           | Effect                                                                                                                                                                   |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `XRAY_ANNOTATIONS` | Set to `"all"` to index every attribute as an X-Ray annotation, or to a space-separated list of attribute keys.                                                          |
| `XRAY_METADATA`    | Set to a space-separated list of attribute keys to restrict metadata to those keys. Defining this variable with an empty value prevents the default export-all behavior. |

Note that attributes prefixed with `annotation.` or `metadata.` are routed according to their prefix regardless of these variables.

### Sampling Strategy

The default sampler is `ParentBased(AlwaysOff)` when the `env-lambda` feature is enabled, and `ParentBased(AlwaysOn)` when it is disabled. The choice is made at compile time, not from the runtime environment.

### Logging

With `tracing-backend` enabled, console logging is driven by the `RUST_LOG` environment variable (via `tracing-subscriber`'s `EnvFilter`). Logs are emitted as structured JSON to stdout, suitable for CloudWatch Logs ingestion. With `otel-backend` only, no console layer is installed.

### API Documentation

Full API documentation is available on [docs.rs](https://docs.rs/awssdk-instrumentation/latest/awssdk_instrumentation).

## Minimum Supported Rust Version (MSRV)

This crate requires **Rust 1.88.0** or later. The MSRV is verified in CI on every push.

## FAQ

**Can I use both backends at once?**

Both `tracing-backend` and `otel-backend` can be enabled simultaneously — they compile side by side. However, `DefaultInterceptor` and `DefaultTracingLayer` always resolve to the `tracing-backend` types when both are active. The `tracing-backend` is recommended for most use cases.

**Why is the default sampler `AlwaysOff` on Lambda?**

On Lambda, the X-Ray service controls sampling via the `_X_AMZN_TRACE_ID` header injected into each invocation. The `ParentBased(AlwaysOff)` sampler means the crate respects the parent sampling decision from X-Ray and does not create additional root traces on its own.

**How do I add extraction for a service not yet supported?**

Implement the `AttributeExtractor` trait and register it on the interceptor's `extractor` field with `register_attribute_extractor()`. For simpler cases, use `register_input_hook()` (or the other `register_*_hook` methods) with a `ServiceFilter` to scope the hook to specific services or operations.

**Do I need to add `opentelemetry` or `tracing` to my own `Cargo.toml`?**

No — both are re-exported. Reach them via `awssdk_instrumentation::opentelemetry` and `awssdk_instrumentation::tracing`. Note that `tracing`, `tracing-subscriber` and `tracing-opentelemetry` are only re-exported under the `tracing-backend` feature (on by default), so a build that enables `otel-backend` alone would need its own `tracing` dependency to use it. `opentelemetry_sdk` and `opentelemetry-semantic-conventions` are always re-exported. If you prefer adding them as direct dependencies for shorter `use` paths, that works too.

**Can I use this crate outside of Lambda?**

Yes. The Lambda-specific functionality is behind the `env-lambda` feature flag. You can disable it and use the other environment
detectors features (`env-ecs`, `env-eks`, `env-ec2`). The AWS SDK instrumentation is independent of these features and can be used
in any application using the AWS SDK for Rust.

## Contributing

We welcome contributions! Please read our [Contributing Guidelines](CONTRIBUTING.md) before submitting pull requests.

## License

Distributed under the MIT License. See [`LICENSE`](LICENSE) for more information.

## Authors

- Jérémie RODON ([@JeremieRodon](https://github.com/JeremieRodon)) [![LinkedIn](https://img.shields.io/badge/linkedin-0077B5?style=for-the-badge&logo=linkedin&logoColor=white)](https://linkedin.com/in/JeremieRodon) — [RustyServerless](https://github.com/RustyServerless) [rustysl.com](https://rustysl.com/index.html?from=github-lambda-appsync)

If you find this crate useful, please star the repository and share your feedback!
