// Environment resource detection — common types and feature-gated detectors.

use opentelemetry_sdk::Resource;

#[cfg(feature = "env-lambda")]
pub mod lambda;

#[cfg(feature = "env-ecs")]
pub mod ecs;

#[cfg(feature = "env-eks")]
pub mod eks;

#[cfg(feature = "env-ec2")]
pub mod ec2;

#[cfg(feature = "env-lambda")]
pub fn default_resource() -> Resource {
    lambda::lambda_resource()
}

#[cfg(all(feature = "env-ecs", not(feature = "env-lambda")))]
pub fn default_resource() -> Resource {
    ecs::ecs_resource()
}

#[cfg(all(
    feature = "env-eks",
    not(any(feature = "env-lambda", feature = "env-ecs"))
))]
pub fn default_resource() -> Resource {
    eks::eks_resource()
}

#[cfg(all(
    feature = "env-ec2",
    not(any(feature = "env-lambda", feature = "env-ecs", feature = "env-eks"))
))]
pub fn default_resource() -> Resource {
    ec2::ec2_resource()
}

#[cfg(not(any(
    feature = "env-lambda",
    feature = "env-ecs",
    feature = "env-eks",
    feature = "env-ec2"
)))]
pub fn default_resource() -> Resource {
    Resource::builder()
        .with_attributes(
            [Some(KeyValue::new(semco::CLOUD_PROVIDER, "aws"))]
                .into_iter()
                .flatten(),
        )
        .build()
}
