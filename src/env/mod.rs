//! OTel [`Resource`] detection for AWS environments.
//!
//! The central entry point is [`default_resource()`], which probes the runtime
//! environment and returns a [`Resource`] populated with OTel semantic-convention
//! attributes. Detection is attempted in priority order:
//!
//! 1. **Lambda** (`env-lambda`) — checks `AWS_LAMBDA_FUNCTION_NAME`
//! 2. **ECS** (`env-ecs`) — checks `ECS_CONTAINER_METADATA_URI_V4`
//! 3. **EKS** (`env-eks`) — checks for the Kubernetes service-account mount
//! 4. **EC2** (`env-ec2`) — queries IMDSv2 for the instance ID
//!
//! The first detector that succeeds wins. If none match, a minimal resource
//! with `cloud.provider = "aws"` is returned.
//!
//! Each detector is in its own feature-gated sub-module. You can call the
//! per-environment functions directly if you need to bypass the auto-detection
//! logic.
//!
//! [`Resource`]: opentelemetry_sdk::Resource

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

#[cfg(any(feature = "env-ec2", feature = "env-eks"))]
mod imds;

/// Auto-detects the AWS runtime environment and returns an OTel [`Resource`].
///
/// Detection is attempted in priority order, stopping at the first success:
///
/// 1. **Lambda** (`env-lambda`) — checks `AWS_LAMBDA_FUNCTION_NAME`; calls
///    [`lambda::lambda_resource`].
/// 2. **ECS** (`env-ecs`) — checks `ECS_CONTAINER_METADATA_URI_V4`; calls
///    [`ecs::ecs_resource`].
/// 3. **EKS** (`env-eks`) — checks for the Kubernetes service-account mount;
///    calls [`eks::eks_resource`].
/// 4. **EC2** (`env-ec2`) — queries IMDSv2; calls [`ec2::ec2_resource`].
///
/// If no detector succeeds (or no environment feature is enabled), a minimal
/// resource with `cloud.provider = "aws"` is returned.
///
/// This function is called automatically by [`crate::init::default_tracer_provider`].
/// Call it directly only when you need to compose a custom [`SdkTracerProvider`].
///
/// # Examples
///
/// ```no_run
/// use awssdk_instrumentation::env::default_resource;
/// use opentelemetry_sdk::trace::SdkTracerProvider;
///
/// let resource = default_resource();
/// let provider = SdkTracerProvider::builder()
///     .with_resource(resource)
///     .build();
/// ```
///
/// [`Resource`]: opentelemetry_sdk::Resource
/// [`SdkTracerProvider`]: opentelemetry_sdk::trace::SdkTracerProvider
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
