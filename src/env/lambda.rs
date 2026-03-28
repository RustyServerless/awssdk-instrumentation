// Lambda ResourceDetector — populates OTel Resource with function name,
// version, memory size, log group, log stream, etc.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::attribute as semco;
use std::env;

pub fn lambda_resource(xray_display_handler_as_lambda: bool) -> Option<Resource> {
    // Check if we're actually running in Lambda
    let function_name = std::env::var("AWS_LAMBDA_FUNCTION_NAME").ok()?;

    Some(
        Resource::builder()
            .with_attributes(
                [
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
                ]
                .into_iter()
                .flatten(),
            )
            .build(),
    )
}
