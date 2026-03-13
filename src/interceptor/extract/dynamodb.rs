// DynamoDB attribute extraction — downcasts Input/Output to concrete
// aws-sdk-dynamodb types and extracts table name, consumed capacity, etc.
//
// Attributes are based on the OpenTelemetry semantic conventions for DynamoDB:
// https://opentelemetry.io/docs/specs/semconv/db/dynamodb/
//
// Attributes that require JSON serialization of complex SDK types containing
// `AttributeValue` are deferred because the SDK model types do not implement
// `serde::Serialize`. These are:
//   - aws.dynamodb.item_collection_metrics (string)
//   - aws.dynamodb.global_secondary_indexes (string[])
//   - aws.dynamodb.local_secondary_indexes (string[])
//   - aws.dynamodb.attribute_definitions (string[])
//   - aws.dynamodb.global_secondary_index_updates (string[])

use std::collections::BTreeSet;

use aws_sdk_dynamodb::operation::{
    batch_get_item::BatchGetItemInput, batch_write_item::BatchWriteItemInput,
    create_backup::CreateBackupInput, create_table::CreateTableInput, delete_item::DeleteItemInput,
    delete_table::DeleteTableInput, describe_continuous_backups::DescribeContinuousBackupsInput,
    describe_contributor_insights::DescribeContributorInsightsInput,
    describe_kinesis_streaming_destination::DescribeKinesisStreamingDestinationInput,
    describe_table::DescribeTableInput,
    describe_table_replica_auto_scaling::DescribeTableReplicaAutoScalingInput,
    describe_time_to_live::DescribeTimeToLiveInput,
    disable_kinesis_streaming_destination::DisableKinesisStreamingDestinationInput,
    enable_kinesis_streaming_destination::EnableKinesisStreamingDestinationInput,
    get_item::GetItemInput, list_backups::ListBackupsInput,
    list_contributor_insights::ListContributorInsightsInput, list_tables::ListTablesInput,
    put_item::PutItemInput, query::QueryInput, scan::ScanInput,
    transact_get_items::TransactGetItemsInput, transact_write_items::TransactWriteItemsInput,
    update_continuous_backups::UpdateContinuousBackupsInput,
    update_contributor_insights::UpdateContributorInsightsInput, update_item::UpdateItemInput,
    update_kinesis_streaming_destination::UpdateKinesisStreamingDestinationInput,
    update_table::UpdateTableInput,
    update_table_replica_auto_scaling::UpdateTableReplicaAutoScalingInput,
    update_time_to_live::UpdateTimeToLiveInput,
};
use aws_sdk_dynamodb::operation::{
    batch_get_item::BatchGetItemOutput, batch_write_item::BatchWriteItemOutput,
    delete_item::DeleteItemOutput, get_item::GetItemOutput, list_tables::ListTablesOutput,
    put_item::PutItemOutput, query::QueryOutput, scan::ScanOutput,
    transact_get_items::TransactGetItemsOutput, transact_write_items::TransactWriteItemsOutput,
    update_item::UpdateItemOutput,
};
use aws_sdk_dynamodb::types;
use aws_smithy_runtime_api::client::interceptors::context;
use opentelemetry::{Array, StringValue, Value};
use opentelemetry_semantic_conventions::attribute as semco;
use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};

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
        span.set_attribute(crate::interceptor::DB_SYSTEM_NAME, "aws.dynamodb");
        match operation {
            // Operations with per-operation helpers (semconv defines extra attributes)
            "GetItem" => extract_get_item_input(input, span),
            "Query" => extract_query_input(input, span),
            "Scan" => extract_scan_input(input, span),
            "CreateTable" => extract_create_table_input(input, span),
            "UpdateTable" => extract_update_table_input(input, span),
            "ListTables" => extract_list_tables_input(input, span),
            "BatchGetItem" => extract_batch_get_item_input(input, span),
            "BatchWriteItem" => extract_batch_write_item_input(input, span),
            "TransactGetItems" => extract_transact_get_items_input(input, span),
            "TransactWriteItems" => extract_transact_write_items_input(input, span),
            // Operations that only have table_name
            "PutItem" => set_table_names(
                span,
                input
                    .downcast_ref::<PutItemInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "UpdateItem" => set_table_names(
                span,
                input
                    .downcast_ref::<UpdateItemInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "DeleteItem" => set_table_names(
                span,
                input
                    .downcast_ref::<DeleteItemInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "DeleteTable" => set_table_names(
                span,
                input
                    .downcast_ref::<DeleteTableInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "DescribeTable" => set_table_names(
                span,
                input
                    .downcast_ref::<DescribeTableInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "DescribeTimeToLive" => set_table_names(
                span,
                input
                    .downcast_ref::<DescribeTimeToLiveInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "UpdateTimeToLive" => set_table_names(
                span,
                input
                    .downcast_ref::<UpdateTimeToLiveInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "DescribeContinuousBackups" => set_table_names(
                span,
                input
                    .downcast_ref::<DescribeContinuousBackupsInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "UpdateContinuousBackups" => set_table_names(
                span,
                input
                    .downcast_ref::<UpdateContinuousBackupsInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "DescribeContributorInsights" => set_table_names(
                span,
                input
                    .downcast_ref::<DescribeContributorInsightsInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "UpdateContributorInsights" => set_table_names(
                span,
                input
                    .downcast_ref::<UpdateContributorInsightsInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "ListContributorInsights" => set_table_names(
                span,
                input
                    .downcast_ref::<ListContributorInsightsInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "DescribeKinesisStreamingDestination" => set_table_names(
                span,
                input
                    .downcast_ref::<DescribeKinesisStreamingDestinationInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "EnableKinesisStreamingDestination" => set_table_names(
                span,
                input
                    .downcast_ref::<EnableKinesisStreamingDestinationInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "DisableKinesisStreamingDestination" => set_table_names(
                span,
                input
                    .downcast_ref::<DisableKinesisStreamingDestinationInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "UpdateKinesisStreamingDestination" => set_table_names(
                span,
                input
                    .downcast_ref::<UpdateKinesisStreamingDestinationInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "CreateBackup" => set_table_names(
                span,
                input
                    .downcast_ref::<CreateBackupInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "ListBackups" => set_table_names(
                span,
                input
                    .downcast_ref::<ListBackupsInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "DescribeTableReplicaAutoScaling" => set_table_names(
                span,
                input
                    .downcast_ref::<DescribeTableReplicaAutoScalingInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            "UpdateTableReplicaAutoScaling" => set_table_names(
                span,
                input
                    .downcast_ref::<UpdateTableReplicaAutoScalingInput>()
                    .expect("correct type")
                    .table_name(),
            ),
            _ => {}
        };
    }

    fn extract_output(
        &self,
        _service: crate::interceptor::Service,
        operation: crate::interceptor::Operation,
        output: &context::Output,
        span: &mut SW,
    ) {
        match operation {
            "Query" => extract_query_output(output, span),
            "Scan" => extract_scan_output(output, span),
            "ListTables" => extract_list_tables_output(output, span),
            "GetItem" => set_consumed_capacity_opt(
                span,
                output
                    .downcast_ref::<GetItemOutput>()
                    .expect("correct type")
                    .consumed_capacity(),
            ),
            "PutItem" => set_consumed_capacity_opt(
                span,
                output
                    .downcast_ref::<PutItemOutput>()
                    .expect("correct type")
                    .consumed_capacity(),
            ),
            "DeleteItem" => set_consumed_capacity_opt(
                span,
                output
                    .downcast_ref::<DeleteItemOutput>()
                    .expect("correct type")
                    .consumed_capacity(),
            ),
            "UpdateItem" => set_consumed_capacity_opt(
                span,
                output
                    .downcast_ref::<UpdateItemOutput>()
                    .expect("correct type")
                    .consumed_capacity(),
            ),
            "BatchGetItem" => set_consumed_capacity_list(
                span,
                output
                    .downcast_ref::<BatchGetItemOutput>()
                    .expect("correct type")
                    .consumed_capacity(),
            ),
            "BatchWriteItem" => set_consumed_capacity_list(
                span,
                output
                    .downcast_ref::<BatchWriteItemOutput>()
                    .expect("correct type")
                    .consumed_capacity(),
            ),
            "TransactGetItems" => set_consumed_capacity_list(
                span,
                output
                    .downcast_ref::<TransactGetItemsOutput>()
                    .expect("correct type")
                    .consumed_capacity(),
            ),
            "TransactWriteItems" => set_consumed_capacity_list(
                span,
                output
                    .downcast_ref::<TransactWriteItemsOutput>()
                    .expect("correct type")
                    .consumed_capacity(),
            ),
            _ => {}
        };
    }
}

// ---------------------------------------------------------------------------
// Per-operation input helpers
// ---------------------------------------------------------------------------

fn extract_get_item_input(input: &context::Input, span: &mut impl SpanWrite) {
    let i = input.downcast_ref::<GetItemInput>().expect("correct type");
    set_table_names(span, i.table_name());
    set_consistent_read(span, i.consistent_read());
    set_projection(span, i.projection_expression());
}

fn extract_query_input(input: &context::Input, span: &mut impl SpanWrite) {
    let i = input.downcast_ref::<QueryInput>().expect("correct type");
    set_table_names(span, i.table_name());
    set_consistent_read(span, i.consistent_read());
    set_projection(span, i.projection_expression());
    set_index_name(span, i.index_name());
    set_select(span, i.select());
    set_limit(span, i.limit());
    set_attributes_to_get(span, i.attributes_to_get());
    if let Some(scan_forward) = i.scan_index_forward() {
        span.set_attribute(semco::AWS_DYNAMODB_SCAN_FORWARD, scan_forward);
    }
}

fn extract_scan_input(input: &context::Input, span: &mut impl SpanWrite) {
    let i = input.downcast_ref::<ScanInput>().expect("correct type");
    set_table_names(span, i.table_name());
    set_consistent_read(span, i.consistent_read());
    set_projection(span, i.projection_expression());
    set_index_name(span, i.index_name());
    set_select(span, i.select());
    set_limit(span, i.limit());
    set_attributes_to_get(span, i.attributes_to_get());
    if let Some(segment) = i.segment() {
        span.set_attribute(semco::AWS_DYNAMODB_SEGMENT, Value::I64(i64::from(segment)));
    }
    if let Some(total_segments) = i.total_segments() {
        span.set_attribute(
            semco::AWS_DYNAMODB_TOTAL_SEGMENTS,
            Value::I64(i64::from(total_segments)),
        );
    }
}

fn extract_create_table_input(input: &context::Input, span: &mut impl SpanWrite) {
    let i = input
        .downcast_ref::<CreateTableInput>()
        .expect("correct type");
    set_table_names(span, i.table_name());
    set_provisioned_throughput(span, i.provisioned_throughput());
}

fn extract_update_table_input(input: &context::Input, span: &mut impl SpanWrite) {
    let i = input
        .downcast_ref::<UpdateTableInput>()
        .expect("correct type");
    set_table_names(span, i.table_name());
    set_provisioned_throughput(span, i.provisioned_throughput());
}

fn extract_list_tables_input(input: &context::Input, span: &mut impl SpanWrite) {
    let i = input
        .downcast_ref::<ListTablesInput>()
        .expect("correct type");
    set_limit(span, i.limit());
    if let Some(exclusive_start) = i.exclusive_start_table_name() {
        span.set_attribute(
            semco::AWS_DYNAMODB_EXCLUSIVE_START_TABLE,
            exclusive_start.to_owned(),
        );
    }
}

fn extract_batch_get_item_input(input: &context::Input, span: &mut impl SpanWrite) {
    set_table_names(
        span,
        input
            .downcast_ref::<BatchGetItemInput>()
            .expect("correct type")
            .request_items()
            .into_iter()
            .flat_map(|map| map.keys())
            .map(|s| s.as_str()),
    );
}

fn extract_batch_write_item_input(input: &context::Input, span: &mut impl SpanWrite) {
    set_table_names(
        span,
        input
            .downcast_ref::<BatchWriteItemInput>()
            .expect("correct type")
            .request_items()
            .into_iter()
            .flat_map(|map| map.keys())
            .map(|s| s.as_str()),
    );
}

fn extract_transact_get_items_input(input: &context::Input, span: &mut impl SpanWrite) {
    set_table_names(
        span,
        input
            .downcast_ref::<TransactGetItemsInput>()
            .expect("correct type")
            .transact_items()
            .iter()
            .filter_map(|item| item.get())
            .map(|get| get.table_name())
            .collect::<BTreeSet<_>>(),
    );
}

fn extract_transact_write_items_input(input: &context::Input, span: &mut impl SpanWrite) {
    set_table_names(
        span,
        input
            .downcast_ref::<TransactWriteItemsInput>()
            .expect("correct type")
            .transact_items()
            .iter()
            .filter_map(|item| {
                item.condition_check()
                    .map(|c| c.table_name())
                    .or_else(|| item.put().map(|p| p.table_name()))
                    .or_else(|| item.delete().map(|d| d.table_name()))
                    .or_else(|| item.update().map(|u| u.table_name()))
            })
            .collect::<BTreeSet<_>>(),
    );
}

// ---------------------------------------------------------------------------
// Per-operation output helpers
// ---------------------------------------------------------------------------

fn extract_query_output(output: &context::Output, span: &mut impl SpanWrite) {
    let o = output.downcast_ref::<QueryOutput>().expect("correct type");
    span.set_attribute(semco::AWS_DYNAMODB_COUNT, Value::I64(i64::from(o.count())));
    span.set_attribute(
        semco::AWS_DYNAMODB_SCANNED_COUNT,
        Value::I64(i64::from(o.scanned_count())),
    );
    set_consumed_capacity_opt(span, o.consumed_capacity());
}

fn extract_scan_output(output: &context::Output, span: &mut impl SpanWrite) {
    let o = output.downcast_ref::<ScanOutput>().expect("correct type");
    span.set_attribute(semco::AWS_DYNAMODB_COUNT, Value::I64(i64::from(o.count())));
    span.set_attribute(
        semco::AWS_DYNAMODB_SCANNED_COUNT,
        Value::I64(i64::from(o.scanned_count())),
    );
    set_consumed_capacity_opt(span, o.consumed_capacity());
}

fn extract_list_tables_output(output: &context::Output, span: &mut impl SpanWrite) {
    let o = output
        .downcast_ref::<ListTablesOutput>()
        .expect("correct type");
    span.set_attribute(
        semco::AWS_DYNAMODB_TABLE_COUNT,
        Value::I64(o.table_names().len() as i64),
    );
}

// ---------------------------------------------------------------------------
// Shared attribute helpers
// ---------------------------------------------------------------------------

fn set_table_names<'a>(span: &mut impl SpanWrite, table_names: impl IntoIterator<Item = &'a str>) {
    let table_names = table_names
        .into_iter()
        .map(|table_name| StringValue::from(table_name.to_owned()))
        .collect::<Vec<_>>();
    if !table_names.is_empty() {
        span.set_attribute(
            semco::AWS_DYNAMODB_TABLE_NAMES,
            Value::Array(Array::String(table_names)),
        );
    }
}

fn set_consistent_read(span: &mut impl SpanWrite, consistent_read: Option<bool>) {
    if let Some(consistent_read) = consistent_read {
        span.set_attribute(semco::AWS_DYNAMODB_CONSISTENT_READ, consistent_read);
    }
}

fn set_projection(span: &mut impl SpanWrite, projection_expression: Option<&str>) {
    if let Some(projection) = projection_expression {
        span.set_attribute(semco::AWS_DYNAMODB_PROJECTION, projection.to_owned());
    }
}

fn set_index_name(span: &mut impl SpanWrite, index_name: Option<&str>) {
    if let Some(index_name) = index_name {
        span.set_attribute(semco::AWS_DYNAMODB_INDEX_NAME, index_name.to_owned());
    }
}

fn set_select(span: &mut impl SpanWrite, select: Option<&aws_sdk_dynamodb::types::Select>) {
    if let Some(select) = select {
        span.set_attribute(semco::AWS_DYNAMODB_SELECT, select.as_str().to_owned());
    }
}

fn set_limit(span: &mut impl SpanWrite, limit: Option<i32>) {
    if let Some(limit) = limit {
        span.set_attribute(semco::AWS_DYNAMODB_LIMIT, Value::I64(i64::from(limit)));
    }
}

fn set_attributes_to_get(span: &mut impl SpanWrite, attributes: &[String]) {
    if !attributes.is_empty() {
        span.set_attribute(
            semco::AWS_DYNAMODB_ATTRIBUTES_TO_GET,
            Value::Array(Array::String(
                attributes
                    .iter()
                    .map(|a| StringValue::from(a.clone()))
                    .collect(),
            )),
        );
    }
}

fn set_provisioned_throughput(
    span: &mut impl SpanWrite,
    throughput: Option<&aws_sdk_dynamodb::types::ProvisionedThroughput>,
) {
    if let Some(pt) = throughput {
        span.set_attribute(
            semco::AWS_DYNAMODB_PROVISIONED_READ_CAPACITY,
            Value::F64(pt.read_capacity_units() as f64),
        );
        span.set_attribute(
            semco::AWS_DYNAMODB_PROVISIONED_WRITE_CAPACITY,
            Value::F64(pt.write_capacity_units() as f64),
        );
    }
}

// ---------------------------------------------------------------------------
// ConsumedCapacity serialization
// ---------------------------------------------------------------------------
//
// The AWS SDK types do not implement `serde::Serialize`. We provide thin
// newtype wrappers with custom `Serialize` impls so we can call
// `serde_json::to_string` and get the JSON format expected by the semconv.

// Sets the `aws.dynamodb.consumed_capacity` attribute from a single optional
// `ConsumedCapacity` value (GetItem, PutItem, DeleteItem, UpdateItem, Query, Scan).
fn set_consumed_capacity_opt(span: &mut impl SpanWrite, cc: Option<&types::ConsumedCapacity>) {
    if let Some(cc) = cc {
        if let Ok(json) = serde_json::to_string(&SerConsumedCapacity(cc)) {
            span.set_attribute(
                semco::AWS_DYNAMODB_CONSUMED_CAPACITY,
                Value::Array(Array::String(vec![StringValue::from(json)])),
            );
        }
    }
}

// Sets the `aws.dynamodb.consumed_capacity` attribute from a list of
// `ConsumedCapacity` values (BatchGetItem, BatchWriteItem, TransactGetItems,
// TransactWriteItems).
fn set_consumed_capacity_list(span: &mut impl SpanWrite, ccs: &[types::ConsumedCapacity]) {
    if !ccs.is_empty() {
        let items: Vec<StringValue> = ccs
            .iter()
            .filter_map(|cc| serde_json::to_string(&SerConsumedCapacity(cc)).ok())
            .map(StringValue::from)
            .collect();
        if !items.is_empty() {
            span.set_attribute(
                semco::AWS_DYNAMODB_CONSUMED_CAPACITY,
                Value::Array(Array::String(items)),
            );
        }
    }
}

// Newtype wrapper for `types::Capacity` that implements `Serialize`.
struct SerCapacity<'a>(&'a types::Capacity);

impl Serialize for SerCapacity<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let cap = self.0;
        let count = cap.capacity_units().is_some() as usize
            + cap.read_capacity_units().is_some() as usize
            + cap.write_capacity_units().is_some() as usize;
        let mut map = serializer.serialize_map(Some(count))?;
        if let Some(cu) = cap.capacity_units() {
            map.serialize_entry("CapacityUnits", &cu)?;
        }
        if let Some(rcu) = cap.read_capacity_units() {
            map.serialize_entry("ReadCapacityUnits", &rcu)?;
        }
        if let Some(wcu) = cap.write_capacity_units() {
            map.serialize_entry("WriteCapacityUnits", &wcu)?;
        }
        map.end()
    }
}

// Newtype wrapper for `types::ConsumedCapacity` that implements `Serialize`.
struct SerConsumedCapacity<'a>(&'a types::ConsumedCapacity);

impl Serialize for SerConsumedCapacity<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let cc = self.0;
        let count = cc.table_name().is_some() as usize
            + cc.capacity_units().is_some() as usize
            + cc.read_capacity_units().is_some() as usize
            + cc.write_capacity_units().is_some() as usize
            + cc.table().is_some() as usize
            + cc.local_secondary_indexes().is_some() as usize
            + cc.global_secondary_indexes().is_some() as usize;
        let mut map = serializer.serialize_map(Some(count))?;
        if let Some(table_name) = cc.table_name() {
            map.serialize_entry("TableName", table_name)?;
        }
        if let Some(cu) = cc.capacity_units() {
            map.serialize_entry("CapacityUnits", &cu)?;
        }
        if let Some(rcu) = cc.read_capacity_units() {
            map.serialize_entry("ReadCapacityUnits", &rcu)?;
        }
        if let Some(wcu) = cc.write_capacity_units() {
            map.serialize_entry("WriteCapacityUnits", &wcu)?;
        }
        if let Some(table) = cc.table() {
            map.serialize_entry("Table", &SerCapacity(table))?;
        }
        if let Some(lsi) = cc.local_secondary_indexes() {
            map.serialize_entry("LocalSecondaryIndexes", &SerCapacityMap(lsi))?;
        }
        if let Some(gsi) = cc.global_secondary_indexes() {
            map.serialize_entry("GlobalSecondaryIndexes", &SerCapacityMap(gsi))?;
        }
        map.end()
    }
}

// Newtype wrapper for a `HashMap<String, Capacity>` that serializes each
// value through `SerCapacity`.
struct SerCapacityMap<'a>(&'a std::collections::HashMap<String, types::Capacity>);

impl Serialize for SerCapacityMap<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, cap) in self.0 {
            map.serialize_entry(key, &SerCapacity(cap))?;
        }
        map.end()
    }
}
