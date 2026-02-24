// DynamoDB attribute extraction — downcasts Input/Output to concrete
// aws-sdk-dynamodb types and extracts table name, consumed capacity, etc.

use aws_sdk_dynamodb::operation::{
    create_table::CreateTableInput, delete_item::DeleteItemInput, delete_table::DeleteTableInput,
    get_item::GetItemInput, put_item::PutItemInput, query::QueryInput, scan::ScanInput,
    update_item::UpdateItemInput,
};
use aws_smithy_runtime_api::client::interceptors::context;
use opentelemetry::{Array, StringValue, Value};
use opentelemetry_semantic_conventions::attribute as semco;

use super::super::{AttributeExtractor, SpanWrite};

#[derive(Debug, Default)]
pub struct DynamoDBExtractor {
    _private: (),
}

impl DynamoDBExtractor {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl<SW: SpanWrite> AttributeExtractor<SW> for DynamoDBExtractor {
    fn extract_input(
        &self,
        _service: crate::interceptor::Service,
        operation: crate::interceptor::Operation,
        input: &context::Input,
        span: &mut SW,
    ) {
        span.set_attribute(semco::DB_SYSTEM_NAME, "aws.dynamodb");
        // Extract and add the table_name for every operation that have it
        let table_name = match operation {
            "GetItem" => input
                .downcast_ref::<GetItemInput>()
                .expect("correct type")
                .table_name(),
            "PutItem" => input
                .downcast_ref::<PutItemInput>()
                .expect("correct type")
                .table_name(),
            "UpdateItem" => input
                .downcast_ref::<UpdateItemInput>()
                .expect("correct type")
                .table_name(),
            "DeleteItem" => input
                .downcast_ref::<DeleteItemInput>()
                .expect("correct type")
                .table_name(),
            "Query" => input
                .downcast_ref::<QueryInput>()
                .expect("correct type")
                .table_name(),
            "Scan" => input
                .downcast_ref::<ScanInput>()
                .expect("correct type")
                .table_name(),
            "CreateTable" => input
                .downcast_ref::<CreateTableInput>()
                .expect("correct type")
                .table_name(),
            "DeleteTable" => input
                .downcast_ref::<DeleteTableInput>()
                .expect("correct type")
                .table_name(),
            // Do nothing for other operations
            _ => None,
        };
        if let Some(table_name) = table_name {
            span.set_attribute(
                semco::AWS_DYNAMODB_TABLE_NAMES,
                Value::Array(Array::String(vec![StringValue::from(
                    table_name.to_owned(),
                )])),
            );
        }
    }
}
