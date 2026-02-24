// Lambda ResourceDetector — populates OTel Resource with function name,
// version, memory size, log group, log stream, etc.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::attribute as semco;
use std::env;

pub fn lambda_resource() -> Resource {
    Resource::builder()
        .with_attributes(
            [
                Some(KeyValue::new(semco::CLOUD_PROVIDER, "aws")),
                Some(KeyValue::new(semco::CLOUD_PLATFORM, "aws_lambda")),
                Some(KeyValue::new("telemetry.auto.version", "0.0.1-jro")),
                env::var("AWS_REGION")
                    .ok()
                    .map(|v| KeyValue::new(semco::CLOUD_REGION, v)),
                env::var("AWS_LAMBDA_FUNCTION_NAME")
                    .ok()
                    .map(|v| KeyValue::new(semco::FAAS_NAME, v)),
                env::var("AWS_LAMBDA_FUNCTION_NAME")
                    .ok()
                    .map(|v| KeyValue::new(semco::SERVICE_NAME, v)),
                env::var("AWS_LAMBDA_FUNCTION_VERSION")
                    .ok()
                    .map(|v| KeyValue::new(semco::FAAS_VERSION, v)),
                env::var("AWS_LAMBDA_FUNCTION_MEMORY_SIZE").ok().map(|v| {
                    KeyValue::new(
                        semco::FAAS_MAX_MEMORY,
                        v.parse::<i64>().unwrap() * 1024 * 1024,
                    )
                }),
                env::var("AWS_LAMBDA_LOG_STREAM_NAME")
                    .ok()
                    .map(|v| KeyValue::new(semco::FAAS_INSTANCE, v)),
            ]
            .into_iter()
            .flatten(),
        )
        .build()
}
