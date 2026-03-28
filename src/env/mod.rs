// Environment resource detection — common types and feature-gated detectors.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::attribute as semco;

#[cfg(feature = "env-lambda")]
pub mod lambda;

#[cfg(feature = "env-ecs")]
pub mod ecs;

#[cfg(feature = "env-eks")]
pub mod eks;

#[cfg(feature = "env-ec2")]
pub mod ec2;

pub fn default_resource() -> Resource {
    #[cfg(feature = "env-lambda")]
    if let Some(resource) = lambda::lambda_resource(false) {
        return resource;
    }

    #[cfg(feature = "env-ecs")]
    if let Some(resource) = ecs::ecs_resource() {
        return resource;
    }

    #[cfg(feature = "env-eks")]
    if let Some(resource) = eks::eks_resource() {
        return resource;
    }

    #[cfg(feature = "env-ec2")]
    if let Some(resource) = ec2::ec2_resource() {
        return resource;
    }

    // Fallback: no env feature enabled or detection failed
    Resource::builder()
        .with_attributes([KeyValue::new(semco::CLOUD_PROVIDER, "aws")])
        .build()
}
