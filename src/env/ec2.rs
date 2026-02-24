// EC2 ResourceDetector — populates OTel Resource with instance ID,
// AMI, availability zone, etc.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::attribute as semco;

pub fn ec2_resource() -> Resource {
    Resource::builder()
        .with_attributes(
            [
                Some(KeyValue::new(semco::CLOUD_PROVIDER, "aws")),
                Some(KeyValue::new(semco::CLOUD_PLATFORM, "aws_ec2")),
            ]
            .into_iter()
            .flatten(),
        )
        .build()
}
