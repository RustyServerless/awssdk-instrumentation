// ECS ResourceDetector — populates OTel Resource with cluster, task ARN,
// container ID, etc.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::attribute as semco;

pub fn ecs_resource() -> Option<Resource> {
    let metadata_uri = std::env::var("ECS_CONTAINER_METADATA_URI_V4").ok()?;

    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return None,
    };

    let task: EcsTaskMetadata = match client.get(format!("{}/task", metadata_uri)).send() {
        Ok(resp) => match resp.json() {
            Ok(task) => task,
            Err(_) => return None,
        },
        Err(_) => return None,
    };

    let mut attributes = vec![
        KeyValue::new(semco::CLOUD_PROVIDER, "aws"),
        KeyValue::new(semco::CLOUD_PLATFORM, "aws_ecs"),
    ];

    if let Some(cluster) = task.cluster {
        attributes.push(KeyValue::new(semco::AWS_ECS_CLUSTER_ARN, cluster));
    }

    if let Some(task_arn) = task.task_arn {
        attributes.push(KeyValue::new(semco::AWS_ECS_TASK_ARN, task_arn.clone()));

        // Extract account ID from task ARN: arn:aws:ecs:region:account-id:task/cluster-name/task-id
        if let Some(account_id) = task_arn.split(':').nth(4) {
            attributes.push(KeyValue::new(
                semco::CLOUD_ACCOUNT_ID,
                account_id.to_owned(),
            ));
        }

        // Extract region from task ARN
        if let Some(region) = task_arn.split(':').nth(3) {
            attributes.push(KeyValue::new(semco::CLOUD_REGION, region.to_owned()));
        }
    }

    if let Some(family) = task.family {
        attributes.push(KeyValue::new(semco::AWS_ECS_TASK_FAMILY, family));
    }

    if let Some(revision) = task.revision {
        attributes.push(KeyValue::new(semco::AWS_ECS_TASK_REVISION, revision));
    }

    if let Some(container_instance_id) = task.container_instance_arn {
        attributes.push(KeyValue::new(
            semco::AWS_ECS_CONTAINER_ARN,
            container_instance_id,
        ));
    }

    // Containers
    for container in task.containers.unwrap_or_default() {
        if let Some(id) = container.id {
            attributes.push(KeyValue::new(semco::CONTAINER_ID, id));
            break;
        }
    }

    Some(Resource::builder().with_attributes(attributes).build())
}

#[derive(serde::Deserialize, Debug)]
struct EcsTaskMetadata {
    #[serde(rename = "Cluster")]
    cluster: Option<String>,
    #[serde(rename = "TaskARN")]
    task_arn: Option<String>,
    #[serde(rename = "Family")]
    family: Option<String>,
    #[serde(rename = "Revision")]
    revision: Option<String>,
    #[serde(rename = "ContainerInstanceARN")]
    container_instance_arn: Option<String>,
    #[serde(rename = "Containers")]
    containers: Option<Vec<EcsContainer>>,
}

#[derive(serde::Deserialize, Debug)]
struct EcsContainer {
    #[serde(rename = "Id")]
    id: Option<String>,
}
