// ECS ResourceDetector — populates OTel Resource with cluster, task ARN,
// container ID, etc.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::attribute as semco;

pub fn ecs_resource() -> Option<Resource> {
    let metadata_uri = std::env::var("ECS_CONTAINER_METADATA_URI_V4").ok()?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .ok()?;

    let task: EcsTaskMetadata = client
        .get(format!("{metadata_uri}/task"))
        .send()
        .ok()?
        .json()
        .ok()?;

    // Container-level metadata (ARN and runtime ID) — from the container endpoint
    let container: Option<EcsContainerMetadata> = client
        .get(&metadata_uri)
        .send()
        .ok()
        .and_then(|r| r.json().ok());

    let attribute_options = [
        Some(KeyValue::new(semco::CLOUD_PROVIDER, "aws")),
        Some(KeyValue::new(semco::CLOUD_PLATFORM, "aws_ecs")),
        task.cluster
            .map(|c| KeyValue::new(semco::AWS_ECS_CLUSTER_ARN, c)),
        task.task_arn
            .as_ref()
            .map(|arn| KeyValue::new(semco::AWS_ECS_TASK_ARN, arn.to_owned())),
        task.task_arn
            .as_ref()
            .and_then(|arn| arn.split(':').nth(3))
            .map(|r| KeyValue::new(semco::CLOUD_REGION, r.to_owned())),
        task.task_arn
            .as_ref()
            .and_then(|arn| arn.split(':').nth(4))
            .map(|a| KeyValue::new(semco::CLOUD_ACCOUNT_ID, a.to_owned())),
        task.family
            .map(|f| KeyValue::new(semco::AWS_ECS_TASK_FAMILY, f)),
        task.revision
            .map(|r| KeyValue::new(semco::AWS_ECS_TASK_REVISION, r)),
        container
            .as_ref()
            .and_then(|c| c.container_arn.as_ref())
            .map(|a| KeyValue::new(semco::AWS_ECS_CONTAINER_ARN, a.clone())),
        container
            .and_then(|c| c.docker_id)
            .map(|id| KeyValue::new(semco::CONTAINER_ID, id)),
    ];

    Some(
        Resource::builder()
            .with_attributes(attribute_options.into_iter().flatten())
            .build(),
    )
}

#[derive(serde::Deserialize)]
struct EcsTaskMetadata {
    #[serde(rename = "Cluster")]
    cluster: Option<String>,
    #[serde(rename = "TaskARN")]
    task_arn: Option<String>,
    #[serde(rename = "Family")]
    family: Option<String>,
    #[serde(rename = "Revision")]
    revision: Option<String>,
}

// Per-container metadata (from GET $ECS_CONTAINER_METADATA_URI_V4 without /task)
#[derive(serde::Deserialize)]
struct EcsContainerMetadata {
    #[serde(rename = "ContainerARN")]
    container_arn: Option<String>,
    #[serde(rename = "DockerId")]
    docker_id: Option<String>,
}
