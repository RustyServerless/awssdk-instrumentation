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

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::Key;
    use opentelemetry_semantic_conventions::attribute as semco;

    // Helper: collect resource attributes into a Vec for inspection
    fn resource_attrs(resource: &Resource) -> Vec<(Key, opentelemetry::Value)> {
        resource
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn find_attr<'a>(
        attrs: &'a [(Key, opentelemetry::Value)],
        key: &str,
    ) -> Option<&'a opentelemetry::Value> {
        attrs
            .iter()
            .find(|(k, _)| k.as_str() == key)
            .map(|(_, v)| v)
    }

    // NOTE: These two tests modify process-wide environment variables and are
    // NOT thread-safe. They are consolidated into a single test function to
    // avoid parallel execution issues when `cargo test` runs tests concurrently.
    #[test]
    fn lambda_resource_env_var_scenarios() {
        // --- Scenario 1: AWS_LAMBDA_FUNCTION_NAME is NOT set ---
        // Save and remove the variable to ensure a clean state.
        let saved_name = std::env::var("AWS_LAMBDA_FUNCTION_NAME").ok();
        unsafe {
            std::env::remove_var("AWS_LAMBDA_FUNCTION_NAME");
        }

        let result = lambda_resource(false);
        assert!(
            result.is_none(),
            "Expected None when AWS_LAMBDA_FUNCTION_NAME is not set"
        );

        // --- Scenario 2: AWS_LAMBDA_FUNCTION_NAME IS set ---
        // Save existing values so we can restore them afterwards.
        let saved_region = std::env::var("AWS_REGION").ok();
        let saved_version = std::env::var("AWS_LAMBDA_FUNCTION_VERSION").ok();
        let saved_memory = std::env::var("AWS_LAMBDA_FUNCTION_MEMORY_SIZE").ok();
        let saved_stream = std::env::var("AWS_LAMBDA_LOG_STREAM_NAME").ok();

        unsafe {
            std::env::set_var("AWS_LAMBDA_FUNCTION_NAME", "test-function");
            std::env::set_var("AWS_REGION", "us-east-1");
            std::env::set_var("AWS_LAMBDA_FUNCTION_VERSION", "$LATEST");
            std::env::set_var("AWS_LAMBDA_FUNCTION_MEMORY_SIZE", "128");
            std::env::set_var("AWS_LAMBDA_LOG_STREAM_NAME", "2024/01/01/[$LATEST]abc123");
        }

        // Test with xray_display_handler_as_lambda = false
        let result = lambda_resource(false);
        assert!(
            result.is_some(),
            "Expected Some when AWS_LAMBDA_FUNCTION_NAME is set"
        );
        let resource = result.unwrap();
        let attrs = resource_attrs(&resource);

        assert_eq!(
            find_attr(&attrs, semco::CLOUD_PROVIDER),
            Some(&opentelemetry::Value::from("aws")),
            "cloud.provider should be 'aws'"
        );
        assert_eq!(
            find_attr(&attrs, semco::FAAS_NAME),
            Some(&opentelemetry::Value::from("test-function")),
            "faas.name should match AWS_LAMBDA_FUNCTION_NAME"
        );
        assert_eq!(
            find_attr(&attrs, semco::SERVICE_NAME),
            Some(&opentelemetry::Value::from("test-function")),
            "service.name should match AWS_LAMBDA_FUNCTION_NAME"
        );
        assert_eq!(
            find_attr(&attrs, semco::CLOUD_REGION),
            Some(&opentelemetry::Value::from("us-east-1")),
            "cloud.region should match AWS_REGION"
        );
        assert_eq!(
            find_attr(&attrs, semco::FAAS_VERSION),
            Some(&opentelemetry::Value::from("$LATEST")),
            "faas.version should match AWS_LAMBDA_FUNCTION_VERSION"
        );
        // 128 MB * 1024 * 1024 = 134217728 bytes
        assert_eq!(
            find_attr(&attrs, semco::FAAS_MAX_MEMORY),
            Some(&opentelemetry::Value::I64(134_217_728)),
            "faas.max_memory should be memory size in bytes"
        );
        assert_eq!(
            find_attr(&attrs, semco::FAAS_INSTANCE),
            Some(&opentelemetry::Value::from("2024/01/01/[$LATEST]abc123")),
            "faas.instance should match AWS_LAMBDA_LOG_STREAM_NAME"
        );
        // cloud.platform should NOT be present when xray_display_handler_as_lambda = false
        assert!(
            find_attr(&attrs, semco::CLOUD_PLATFORM).is_none(),
            "cloud.platform should be absent when xray_display_handler_as_lambda is false"
        );

        // Test with xray_display_handler_as_lambda = true
        let result_xray = lambda_resource(true);
        assert!(result_xray.is_some());
        let resource_xray = result_xray.unwrap();
        let attrs_xray = resource_attrs(&resource_xray);
        assert_eq!(
            find_attr(&attrs_xray, semco::CLOUD_PLATFORM),
            Some(&opentelemetry::Value::from("aws_lambda")),
            "cloud.platform should be 'aws_lambda' when xray_display_handler_as_lambda is true"
        );

        // Restore environment variables
        unsafe {
            match saved_name {
                Some(v) => std::env::set_var("AWS_LAMBDA_FUNCTION_NAME", v),
                None => std::env::remove_var("AWS_LAMBDA_FUNCTION_NAME"),
            }
            match saved_region {
                Some(v) => std::env::set_var("AWS_REGION", v),
                None => std::env::remove_var("AWS_REGION"),
            }
            match saved_version {
                Some(v) => std::env::set_var("AWS_LAMBDA_FUNCTION_VERSION", v),
                None => std::env::remove_var("AWS_LAMBDA_FUNCTION_VERSION"),
            }
            match saved_memory {
                Some(v) => std::env::set_var("AWS_LAMBDA_FUNCTION_MEMORY_SIZE", v),
                None => std::env::remove_var("AWS_LAMBDA_FUNCTION_MEMORY_SIZE"),
            }
            match saved_stream {
                Some(v) => std::env::set_var("AWS_LAMBDA_LOG_STREAM_NAME", v),
                None => std::env::remove_var("AWS_LAMBDA_LOG_STREAM_NAME"),
            }
        }
    }
}
