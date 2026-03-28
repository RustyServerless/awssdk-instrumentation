// EC2 ResourceDetector — populates OTel Resource with instance ID,
// AMI, availability zone, etc.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::attribute as semco;

pub fn ec2_resource() -> Option<Resource> {
    // Instance ID
    let instance_id = fetch_imds("instance-id").ok()?;

    let mut attributes = vec![
        KeyValue::new(semco::CLOUD_PROVIDER, "aws"),
        KeyValue::new(semco::CLOUD_PLATFORM, "aws_ec2"),
        KeyValue::new(semco::HOST_ID, instance_id),
    ];

    // Instance type
    if let Ok(instance_type) = fetch_imds("instance-type") {
        attributes.push(KeyValue::new(semco::HOST_TYPE, instance_type));
    }

    // AMI ID
    if let Ok(ami_id) = fetch_imds("ami-id") {
        attributes.push(KeyValue::new(semco::HOST_IMAGE_ID, ami_id));
    }

    // Availability zone
    if let Ok(az) = fetch_imds("placement/availability-zone") {
        attributes.push(KeyValue::new(semco::CLOUD_AVAILABILITY_ZONE, az.clone()));

        // Region (derived from AZ, e.g., us-east-1a -> us-east-1)
        if let Some(region) = az.strip_suffix(|c: char| c.is_ascii_alphabetic()) {
            attributes.push(KeyValue::new(semco::CLOUD_REGION, region.to_owned()));
        }
    }

    // Account ID (from IAM role info)
    if let Ok(account) = fetch_imds("identity-credentials/ec2/info/AccountId") {
        attributes.push(KeyValue::new(semco::CLOUD_ACCOUNT_ID, account));
    }

    // VPC ID (optional)
    if let Ok(vpc_id) = fetch_imds("network/interfaces/macs/*/vpc-id") {
        if !vpc_id.is_empty() {
            attributes.push(KeyValue::new("aws.ec2.vpc-id", vpc_id));
        }
    }

    // Subnet ID (optional)
    if let Ok(subnet_id) = fetch_imds("network/interfaces/macs/*/subnet-id") {
        if !subnet_id.is_empty() {
            attributes.push(KeyValue::new("aws.ec2.subnet-id", subnet_id));
        }
    }

    Some(Resource::builder().with_attributes(attributes).build())
}

fn fetch_imds(path: &str) -> Result<String, reqwest::Error> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()?;

    let url = format!("http://169.254.169.254/latest/meta-data/{}", path);
    client.get(&url).send()?.text()
}
