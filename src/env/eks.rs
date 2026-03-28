// EKS ResourceDetector — populates OTel Resource with cluster name,
// pod, namespace, etc.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::attribute as semco;

pub fn eks_resource() -> Option<Resource> {
    if !running_in_k8s() {
        return None;
    }

    let mut attributes = vec![
        KeyValue::new(semco::CLOUD_PROVIDER, "aws"),
        KeyValue::new(semco::CLOUD_PLATFORM, "aws_eks"),
    ];

    // Get pod info from downward API
    if let Ok(namespace) =
        std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/namespace")
    {
        let namespace = namespace.trim();
        if !namespace.is_empty() {
            attributes.push(KeyValue::new(
                semco::K8S_NAMESPACE_NAME,
                namespace.to_owned(),
            ));
        }
    }

    if let Ok(pod_name) =
        std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/pod")
    {
        let pod_name = pod_name.trim();
        if !pod_name.is_empty() {
            attributes.push(KeyValue::new(semco::K8S_POD_NAME, pod_name.to_owned()));
        }
    }

    // Try to get cluster name from EKS DescribeCluster (best effort)
    if let Some(cluster_name) = get_eks_cluster_name() {
        attributes.push(KeyValue::new(semco::K8S_CLUSTER_NAME, cluster_name));
    }

    // Get container ID from cgroup
    if let Some(container_id) = get_container_id() {
        attributes.push(KeyValue::new(semco::CONTAINER_ID, container_id));
    }

    // Get AWS metadata (region, account)
    if let Some(region) = get_aws_region() {
        attributes.push(KeyValue::new(semco::CLOUD_REGION, region));
    }

    if let Some(account_id) = get_aws_account_id() {
        attributes.push(KeyValue::new(semco::CLOUD_ACCOUNT_ID, account_id));
    }

    Some(Resource::builder().with_attributes(attributes).build())
}

fn running_in_k8s() -> bool {
    std::path::Path::new("/var/run/secrets/kubernetes.io/serviceaccount/namespace").exists()
}

fn get_eks_cluster_name() -> Option<String> {
    std::env::var("AWS_CLUSTER_NAME").ok()
}

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

fn get_aws_region() -> Option<String> {
    if let Ok(region) = fetch_imds("latest/meta-data/placement/region") {
        return Some(region);
    }
    std::env::var("AWS_REGION").ok()
}

fn get_aws_account_id() -> Option<String> {
    if let Ok(account) = fetch_imds("latest/meta-data/identity-credentials/ec2/info/AccountId") {
        return Some(account);
    }
    std::env::var("AWS_ACCOUNT_ID").ok()
}

fn fetch_imds(path: &str) -> Result<String, reqwest::Error> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()?;

    let url = format!("http://169.254.169.254/{}", path);
    client.get(&url).send()?.text()
}
