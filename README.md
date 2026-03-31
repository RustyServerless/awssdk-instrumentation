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
    <li><a href="#related-projects">Related Projects</a></li>
  </ol>
</details>

## About The Project

`awssdk-instrumentation` wires together three concerns that every instrumented AWS workload needs:

1. **SDK interceptors** — automatically attach OpenTelemetry semantic-convention attributes to every AWS SDK call (DynamoDB, S3, SQS, and more via user-defined extractors).
2. **Lambda Tower layer** — create a per-invocation span covering the handler, propagate the X-Ray trace context, track cold-starts, and flush the exporter after each invocation.
3. **Environment resource detection** — detect whether the process is running on Lambda, ECS, EKS, or EC2 and populate the OTel `Resource` accordingly.

The default feature set (`tracing-backend` + `env-lambda` + `extract-dynamodb` + `export-xray`) covers the most common Lambda workload with zero extra configuration.

## Features

- Automatic OTel span enrichment for every AWS SDK call (region, operation, HTTP status, request ID, service-specific attributes)
- Per-invocation Lambda spans with X-Ray trace context propagation and cold-start tracking
- Built-in attribute extractors for DynamoDB, S3, and SQS
- Extensible extraction pipeline: register custom `AttributeExtractor` implementations or closure hooks filtered by service/operation
- Auto-detection of AWS runtime environment (Lambda, ECS, EKS, EC2) for OTel `Resource` population
- X-Ray ID generation, propagation, and daemon export out of the box
- `make_lambda_runtime!` macro for zero-boilerplate Lambda setup
- Two backend options: `tracing` ecosystem integration (default) or direct OTel span management

## Getting Started

### Prerequisites

- Rust 1.85.0 or later
- An AWS SDK for Rust client (`aws-sdk-dynamodb`, `aws-sdk-s3`, etc.)
- For Lambda workloads: `lambda_runtime` and `tokio`

### Installation

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
awssdk-instrumentation = "0.1"
```

Or using cargo:

```sh
cargo add awssdk-instrumentation
```

The default features (`tracing-backend`, `env-lambda`, `extract-dynamodb`, `export-xray`) are suitable for most Lambda + DynamoDB workloads. See [Feature Flags](#feature-flags) to customise.

## Usage

> **Note:** This crate does not re-export the AWS SDK crates. You must add
> `aws-config`, `aws-sdk-dynamodb`, `aws-sdk-s3`, or whichever service crates
> you use to your own `Cargo.toml`.

### Quick Start — Lambda with DynamoDB

The `make_lambda_runtime!` macro generates `main()`, telemetry initialisation, SDK client singletons, and the Tower layer in a single call:

```rust
use lambda_runtime::{Error, LambdaEvent};
use serde_json::Value;

// 1. Declare the handler.
async fn handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
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

### Manual Setup

When you need more control over the telemetry stack, wire the pieces together yourself:

```rust
use lambda_runtime::{Error, LambdaEvent};
use serde_json::Value;
use awssdk_instrumentation::{
    init::default_telemetry_init,
    interceptor::DefaultInterceptor,
    lambda::layer::{DefaultTracingLayer, OTelFaasTrigger},
};

async fn handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
    todo!("Do Stuff...");
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialise telemetry (sets global tracer provider + tracing subscriber).
    let tracer_provider = default_telemetry_init();

    // Build an SDK client with the interceptor attached.
    let sdk_config = aws_config::load_from_env().await;
    let dynamo = aws_sdk_dynamodb::Client::from_conf(
        aws_sdk_dynamodb::config::Builder::from(&sdk_config)
            .interceptor(DefaultInterceptor::new())
            .build(),
    );

    // Wrap the Lambda runtime with the Tower layer.
    lambda_runtime::Runtime::new(lambda_runtime::service_fn(handler))
        .layer(
            DefaultTracingLayer::new(move || {
                let _ = tracer_provider.force_flush();
            })
            .with_trigger(OTelFaasTrigger::Http),
        )
        .run()
        .await
}
```

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
use awssdk_instrumentation::interceptor::{AttributeExtractor, Operation, Service};
use awssdk_instrumentation::span_write::SpanWrite;
use aws_smithy_runtime_api::client::interceptors::context;

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

| Feature | Default | Description |
|---|---|---|
| `tracing-backend` | ✅ | Writes span attributes via `tracing::Span` + `tracing-opentelemetry`. Integrates naturally with the `tracing` ecosystem. |
| `otel-backend` | | Manages OTel spans directly without `tracing`. |

### Environment Detection

| Feature | Default | Description |
|---|---|---|
| `env-lambda` | ✅ | Lambda Tower layer, resource detector, `make_lambda_runtime!` macro. |
| `env-ecs` | | ECS resource detector (reads container metadata endpoint). |
| `env-eks` | | EKS resource detector (reads Kubernetes service account + IMDSv2). |
| `env-ec2` | | EC2 resource detector (reads IMDSv2). |

### Service Attribute Extraction

| Feature | Default | Description |
|---|---|---|
| `extract-dynamodb` | ✅ | DynamoDB OTel semantic-convention attributes (table name, consumed capacity, etc.). |
| `extract-s3` | | S3 OTel semantic-convention attributes (bucket name, key, etc.). |
| `extract-sqs` | | SQS OTel semantic-convention attributes (queue URL, message ID, etc.). |

### Export

| Feature | Default | Description |
|---|---|---|
| `export-xray` | ✅ | X-Ray ID generator, propagator, and daemon exporter via `opentelemetry-aws`. |

When `export-xray` is enabled, the `opentelemetry_aws` crate is re-exported at the crate root so you can access the X-Ray propagator and exporter types directly.

## Configuration

### X-Ray Annotations and Metadata

When `export-xray` is enabled, two environment variables control how span attributes are mapped to X-Ray segments:

| Variable | Effect |
|---|---|
| `XRAY_ANNOTATIONS` | Set to `"all"` to index every attribute as an X-Ray annotation, or to a space-separated list of attribute keys. |
| `XRAY_METADATA` | Set to `"all"` to include every attribute as X-Ray metadata, or to a space-separated list of attribute keys. |

### Sampling Strategy

The default sampler is `ParentBased(AlwaysOff)` when `env-lambda` is enabled — Lambda controls sampling via the X-Ray trace header. Outside Lambda, the default is `ParentBased(AlwaysOn)`.

### Logging

Console logging is driven by the `RUST_LOG` environment variable (via `tracing-subscriber`'s `EnvFilter`). Logs are emitted as structured JSON to stdout, suitable for CloudWatch Logs ingestion.

### API Documentation

Full API documentation is available on [docs.rs](https://docs.rs/awssdk-instrumentation/latest/awssdk_instrumentation).

## Minimum Supported Rust Version (MSRV)

This crate requires **Rust 1.85.0** or later. The MSRV is verified in CI on every push.

## FAQ

**Can I use both backends at once?**

Both `tracing-backend` and `otel-backend` can be enabled simultaneously — they compile side by side. However, `DefaultInterceptor` and `DefaultTracingLayer` always resolve to the `tracing-backend` types when both are active. The `tracing-backend` is recommended for most use cases.

**Why is the default sampler `AlwaysOff` on Lambda?**

On Lambda, the X-Ray service controls sampling via the `_X_AMZN_TRACE_ID` header injected into each invocation. The `ParentBased(AlwaysOff)` sampler means the crate respects the parent sampling decision from X-Ray and does not create additional root traces on its own.

**How do I add extraction for a service not yet supported?**

Implement the `AttributeExtractor` trait and register it on the interceptor's `extractor` field with `register_attribute_extractor()`. For simpler cases, use `register_input_hook()` (or the other `register_*_hook` methods) with a `ServiceFilter` to scope the hook to specific services or operations.

**Do I need to add `opentelemetry` or `tracing` to my own `Cargo.toml`?**

Not for basic usage. The `make_lambda_runtime!` macro and `default_telemetry_init()` handle all OTel and tracing setup internally. You only need direct dependencies on these crates if you compose the subscriber stack yourself or interact with OTel/tracing APIs in your handler code.

**Can I use this crate outside of Lambda?**

Yes. The Lambda-specific functionality is behind the `env-lambda` feature flag. Disable it and use the interceptor directly with any AWS SDK client. The environment detectors (`env-ecs`, `env-eks`, `env-ec2`) populate the OTel `Resource` for other AWS compute environments.

## Contributing

We welcome contributions! Please read our [Contributing Guidelines](CONTRIBUTING.md) before submitting pull requests.

## License

Distributed under the MIT License. See [`LICENSE`](LICENSE) for more information.

## Authors

- Jérémie RODON ([@JeremieRodon](https://github.com/JeremieRodon)) [![LinkedIn](https://img.shields.io/badge/linkedin-0077B5?style=for-the-badge&logo=linkedin&logoColor=white)](https://linkedin.com/in/JeremieRodon) — [RustyServerless](https://github.com/RustyServerless) [rustysl.com](https://rustysl.com/index.html?from=github-lambda-appsync)

## Related Projects

- [opentelemetry-rust](https://github.com/open-telemetry/opentelemetry-rust) — The OpenTelemetry SDK for Rust
- [opentelemetry-rust-contrib](https://github.com/open-telemetry/opentelemetry-rust-contrib) — Community-maintained OTel exporters and propagators (X-Ray, etc.)
- [aws-sdk-rust](https://github.com/awslabs/aws-sdk-rust) — The AWS SDK for Rust
- [lambda_runtime](https://github.com/awslabs/aws-lambda-rust-runtime) — The Rust runtime for AWS Lambda

If you find this crate useful, please star the repository and share your feedback!
