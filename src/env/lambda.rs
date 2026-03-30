//! Lambda resource detector (`env-lambda` feature).
//!
//! Reads standard Lambda environment variables and returns an OTel [`Resource`]
//! with the following attributes (when the corresponding variable is set):
//!
//! | OTel attribute              | Source environment variable              |
//! |-----------------------------|------------------------------------------|
//! | `cloud.provider`            | hardcoded `"aws"`                        |
//! | `telemetry.distro.name`     | crate name (compile-time)                |
//! | `telemetry.distro.version`  | crate version (compile-time)             |
//! | `cloud.region`              | `AWS_REGION`                             |
//! | `faas.name` / `service.name`| `AWS_LAMBDA_FUNCTION_NAME`               |
//! | `faas.version`              | `AWS_LAMBDA_FUNCTION_VERSION`            |
//! | `faas.max_memory`           | `AWS_LAMBDA_FUNCTION_MEMORY_SIZE` (bytes)|
//! | `faas.instance`             | `AWS_LAMBDA_LOG_STREAM_NAME`             |
//!
//! Detection succeeds only when `AWS_LAMBDA_FUNCTION_NAME` is set. If the
//! variable is absent, [`lambda_resource()`] returns `None` and
//! [`super::default_resource()`] falls through to the next detector.
//!
//! [`Resource`]: opentelemetry_sdk::Resource

// Lambda ResourceDetector — populates OTel Resource with function name,
// version, memory size, log group, log stream, etc.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::attribute as semco;
use std::env;

/// Builds an OTel [`Resource`] from Lambda environment variables.
///
/// Returns `Some(Resource)` when `AWS_LAMBDA_FUNCTION_NAME` is set (i.e. the
/// process is running inside a Lambda execution environment), or `None`
/// otherwise.
///
/// The `xray_display_handler_as_lambda` parameter controls whether
/// `cloud.platform = "aws_lambda"` is included in the resource. When `true`,
/// X-Ray displays the handler node with a Lambda icon. The default pipeline
/// passes `false` to match the behaviour of the Python ADOT auto-instrumentation.
///
/// See the [module-level documentation](self) for the full attribute table.
///
/// # Examples
///
/// ```no_run
/// use awssdk_instrumentation::env::lambda::lambda_resource;
///
/// // Returns None when not running in Lambda.
/// let resource = lambda_resource(false);
/// ```
///
/// [`Resource`]: opentelemetry_sdk::Resource
pub fn lambda_resource(xray_display_handler_as_lambda: bool) -> Option<Resource> {
    // Check if we're actually running in Lambda
    let function_name = std::env::var("AWS_LAMBDA_FUNCTION_NAME").ok()?;

    let attribute_options = [
        Some(KeyValue::new(semco::CLOUD_PROVIDER, "aws")),
        // This would cause X-Ray to display the Handler node with a Lambda icon
        // The trace graph displays 2 separate nodes with the same icon (the segment coming
        // from the service itself, and the segment of the handler) but the waterfall merges them.
        // I wonder if it as actually desirable.
        // The Python ADOT auto-instrumentation do not do that, for example.
        xray_display_handler_as_lambda
            .then_some(KeyValue::new(semco::CLOUD_PLATFORM, "aws_lambda")),
        Some(KeyValue::new(
            semco::TELEMETRY_DISTRO_NAME,
            env!("CARGO_PKG_NAME"),
        )),
        Some(KeyValue::new(
            semco::TELEMETRY_DISTRO_VERSION,
            env!("CARGO_PKG_VERSION"),
        )),
        env::var("AWS_REGION")
            .ok()
            .map(|v| KeyValue::new(semco::CLOUD_REGION, v)),
        Some(KeyValue::new(semco::FAAS_NAME, function_name.clone())),
        Some(KeyValue::new(semco::SERVICE_NAME, function_name)),
        env::var("AWS_LAMBDA_FUNCTION_VERSION")
            .ok()
            .map(|v| KeyValue::new(semco::FAAS_VERSION, v)),
        env::var("AWS_LAMBDA_FUNCTION_MEMORY_SIZE")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .map(|v| KeyValue::new(semco::FAAS_MAX_MEMORY, v * 1024 * 1024)),
        env::var("AWS_LAMBDA_LOG_STREAM_NAME")
            .ok()
            .map(|v| KeyValue::new(semco::FAAS_INSTANCE, v)),
    ];

    Some(
        Resource::builder()
            .with_attributes(attribute_options.into_iter().flatten())
            .build(),
    )
}
