// ECS ResourceDetector — populates OTel Resource with cluster, task ARN,
// container ID, etc.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::attribute as semco;

pub fn ecs_resource() -> Resource {
    Resource::builder()
        .with_attributes(
            [
                Some(KeyValue::new(semco::CLOUD_PROVIDER, "aws")),
                Some(KeyValue::new(semco::CLOUD_PLATFORM, "aws_ecs")),
            ]
            .into_iter()
            .flatten(),
        )
        .build()
}
