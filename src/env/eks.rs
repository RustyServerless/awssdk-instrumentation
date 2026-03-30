//! EKS resource detector (`env-eks` feature).
//!
//! Detects an EKS environment by checking for the Kubernetes service-account
//! namespace file (`/var/run/secrets/kubernetes.io/serviceaccount/namespace`).
//! When present, it returns an OTel [`Resource`] with the following attributes:
//!
//! | OTel attribute          | Source                                          |
//! |-------------------------|-------------------------------------------------|
//! | `cloud.provider`        | hardcoded `"aws"`                               |
//! | `cloud.platform`        | hardcoded `"aws_eks"`                           |
//! | `k8s.namespace.name`    | service-account namespace file                  |
//! | `k8s.pod.name`          | `HOSTNAME` environment variable                 |
//! | `k8s.cluster.name`      | `AWS_CLUSTER_NAME` environment variable         |
//! | `container.id`          | Docker container ID from `/proc/1/cgroup`       |
//! | `cloud.region`          | IMDSv2 `placement/region`, fallback `AWS_REGION`|
//! | `cloud.account.id`      | IMDSv2 identity credentials, fallback `AWS_ACCOUNT_ID` |
//!
//! [`Resource`]: opentelemetry_sdk::Resource

// EKS ResourceDetector — populates OTel Resource with cluster name,
// pod, namespace, etc.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::attribute as semco;

use super::imds::ImdsClient;

/// Builds an OTel [`Resource`] for an EKS environment.
///
/// Returns `Some(Resource)` when the Kubernetes service-account namespace file
/// (`/var/run/secrets/kubernetes.io/serviceaccount/namespace`) exists, or
/// `None` otherwise.
///
/// When detection succeeds, the function queries IMDSv2 for the AWS region and
/// account ID, falling back to the `AWS_REGION` and `AWS_ACCOUNT_ID`
/// environment variables if IMDS is unreachable.
///
/// See the [module-level documentation](self) for the full attribute table.
///
/// # Examples
///
/// ```no_run
/// use awssdk_instrumentation::env::eks::eks_resource;
///
/// // Returns None when not running in EKS.
/// let resource = eks_resource();
/// ```
///
/// [`Resource`]: opentelemetry_sdk::Resource
pub fn eks_resource() -> Option<Resource> {
    if !running_in_k8s() {
        return None;
    }

    // AWS region and account ID — from IMDS (single client) or environment variables.
    // ImdsClient::new() returns None when not running on EC2/EKS, which is fine — env
    // vars serve as the fallback.
    let imds = ImdsClient::new();

    let attribute_options = [
        Some(KeyValue::new(semco::CLOUD_PROVIDER, "aws")),
        Some(KeyValue::new(semco::CLOUD_PLATFORM, "aws_eks")),
        // Namespace — from the Kubernetes service-account token mount
        std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/namespace")
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .map(|s| KeyValue::new(semco::K8S_NAMESPACE_NAME, s)),
        // Pod name — HOSTNAME is set to the pod name in standard k8s pods
        std::env::var("HOSTNAME")
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .map(|s| KeyValue::new(semco::K8S_POD_NAME, s)),
        // Cluster name — from env var (requires IRSA or manual EKS configuration)
        std::env::var("AWS_CLUSTER_NAME")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| KeyValue::new(semco::K8S_CLUSTER_NAME, s)),
        // Container ID — from cgroup (works for Docker/containerd)
        get_container_id().map(|id| KeyValue::new(semco::CONTAINER_ID, id)),
        // Region — IMDS first, then env var
        imds.as_ref()
            .and_then(|c| c.get("placement/region"))
            .or_else(|| std::env::var("AWS_REGION").ok())
            .map(|r| KeyValue::new(semco::CLOUD_REGION, r)),
        // Account ID — IMDS first, then env var
        imds.as_ref()
            .and_then(|c| c.get_json::<Ec2IdentityCredentials>("identity-credentials/ec2/info"))
            .and_then(|c| c.account_id)
            .or_else(|| std::env::var("AWS_ACCOUNT_ID").ok())
            .map(|a| KeyValue::new(semco::CLOUD_ACCOUNT_ID, a)),
    ];
    Some(
        Resource::builder()
            .with_attributes(attribute_options.into_iter().flatten())
            .build(),
    )
}

/// Returns `true` if the Kubernetes service-account namespace file is present.
fn running_in_k8s() -> bool {
    std::path::Path::new("/var/run/secrets/kubernetes.io/serviceaccount/namespace").exists()
}

/// Reads the Docker container ID from `/proc/1/cgroup` by looking for a `docker/` path segment.
fn get_container_id() -> Option<String> {
    if let Ok(content) = std::fs::read_to_string("/proc/1/cgroup") {
        for line in content.lines() {
            if let Some(id) = line.split(':').next_back() {
                if id.contains("docker/") {
                    return id.split('/').next_back().map(String::from);
                }
            }
        }
    }
    None
}

/// Deserialization target for the IMDSv2 `identity-credentials/ec2/info` JSON response.
#[derive(serde::Deserialize)]
struct Ec2IdentityCredentials {
    #[serde(rename = "AccountId")]
    account_id: Option<String>,
}
