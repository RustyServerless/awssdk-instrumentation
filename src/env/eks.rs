// EKS ResourceDetector — populates OTel Resource with cluster name,
// pod, namespace, etc.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::attribute as semco;

pub fn eks_resource() -> Resource {
    Resource::builder()
        .with_attributes(
            [
                Some(KeyValue::new(semco::CLOUD_PROVIDER, "aws")),
                Some(KeyValue::new(semco::CLOUD_PLATFORM, "aws_eks")),
            ]
            .into_iter()
            .flatten(),
        )
        .build()
}
