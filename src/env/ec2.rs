// EC2 ResourceDetector — populates OTel Resource with instance ID,
// AMI, availability zone, etc.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::attribute as semco;

use super::imds::ImdsClient;

pub fn ec2_resource() -> Option<Resource> {
    let imds = ImdsClient::new()?;

    // Instance ID — required; bail if unavailable (not running on EC2)
    let instance_id = imds.get("instance-id")?;

    let az = imds.get("placement/availability-zone");

    let attribute_options = [
        Some(KeyValue::new(semco::CLOUD_PROVIDER, "aws")),
        Some(KeyValue::new(semco::CLOUD_PLATFORM, "aws_ec2")),
        Some(KeyValue::new(semco::HOST_ID, instance_id)),
        imds.get("instance-type")
            .map(|v| KeyValue::new(semco::HOST_TYPE, v)),
        imds.get("ami-id")
            .map(|v| KeyValue::new(semco::HOST_IMAGE_ID, v)),
        az.as_deref()
            .map(|v| KeyValue::new(semco::CLOUD_AVAILABILITY_ZONE, v.to_owned())),
        // Region — derived from AZ (e.g., us-east-1a -> us-east-1)
        az.as_deref()
            .and_then(|v| v.strip_suffix(|c: char| c.is_ascii_alphabetic()))
            .map(|v| KeyValue::new(semco::CLOUD_REGION, v.to_owned())),
        // Account ID — from the IAM identity document (JSON body, not a sub-path)
        imds.get_json::<Ec2IdentityCredentials>("identity-credentials/ec2/info")
            .and_then(|c| c.account_id)
            .map(|a| KeyValue::new(semco::CLOUD_ACCOUNT_ID, a)),
    ];
    Some(
        Resource::builder()
            .with_attributes(attribute_options.into_iter().flatten())
            .build(),
    )
}

#[derive(serde::Deserialize)]
struct Ec2IdentityCredentials {
    #[serde(rename = "AccountId")]
    account_id: Option<String>,
}
