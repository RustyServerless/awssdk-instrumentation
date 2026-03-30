//! DynamoDB attribute extraction following OTel semantic conventions.
//!
//! This module provides [`DynamoDBExtractor`], which implements
//! [`super::super::AttributeExtractor`] for DynamoDB SDK calls. It is
//! automatically used by [`super::super::DefaultExtractor`] when the
//! `extract-dynamodb` feature is enabled.
//!
//! ## Extracted attributes
//!
//! **Always set:**
//! - `db.system.name` = `"aws.dynamodb"`
//!
//! **Per-operation input attributes** (a subset of the
//! [OTel DynamoDB semconv](https://opentelemetry.io/docs/specs/semconv/db/dynamodb/)):
//! - `aws.dynamodb.table_names` — for all single-table and multi-table operations
//! - `aws.dynamodb.consistent_read`, `aws.dynamodb.projection`,
//!   `aws.dynamodb.index_name`, `aws.dynamodb.select`, `aws.dynamodb.limit`,
//!   `aws.dynamodb.attributes_to_get` — for `GetItem`, `Query`, `Scan`
//! - `aws.dynamodb.scan_forward` — for `Query`
//! - `aws.dynamodb.segment`, `aws.dynamodb.total_segments` — for `Scan`
//! - `aws.dynamodb.provisioned_read_capacity`,
//!   `aws.dynamodb.provisioned_write_capacity` — for `CreateTable`, `UpdateTable`
//! - `aws.dynamodb.exclusive_start_table` — for `ListTables`
//!
//! **Per-operation output attributes:**
//! - `aws.dynamodb.count`, `aws.dynamodb.scanned_count` — for `Query`, `Scan`
//! - `aws.dynamodb.table_count` — for `ListTables`
//! - `aws.dynamodb.consumed_capacity` (JSON array) — for all operations that
//!   return `ConsumedCapacity`
//!
//! ## Deferred attributes
//!
//! The following semconv attributes are not yet extracted because the AWS SDK
//! model types do not implement `serde::Serialize`:
//! - `aws.dynamodb.item_collection_metrics`
//! - `aws.dynamodb.global_secondary_indexes`
//! - `aws.dynamodb.local_secondary_indexes`
//! - `aws.dynamodb.attribute_definitions`
//! - `aws.dynamodb.global_secondary_index_updates`

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

/// Attribute extractor for DynamoDB SDK calls.
///
/// `DynamoDBExtractor` implements [`AttributeExtractor`] and is automatically
/// used by [`DefaultExtractor`] when the `extract-dynamodb` feature is enabled.
/// You only need to construct it directly if you are composing a custom
/// extraction pipeline.
///
/// See the [module-level documentation](self) for the full list of extracted
/// attributes.
///
/// [`DefaultExtractor`]: crate::interceptor::DefaultExtractor
#[derive(Debug, Default)]
pub struct DynamoDBExtractor {
    _private: (),
}

impl DynamoDBExtractor {
    /// Creates a new `DynamoDBExtractor`.
    ///
    /// # Examples
    ///
    /// ```
    /// use awssdk_instrumentation::interceptor::extract::dynamodb::DynamoDBExtractor;
    ///
    /// let extractor = DynamoDBExtractor::new();
    /// ```
    pub fn new() -> Self {
        Self { _private: () }
    }
}

/// Extracts DynamoDB-specific OTel attributes from SDK inputs and outputs.
///
/// See the [module-level documentation](self) for the full list of extracted
/// attributes and which operations they apply to.
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

/// Extracts input attributes for the `GetItem` operation.
fn extract_get_item_input(input: &context::Input, span: &mut impl SpanWrite) {
    let i = input.downcast_ref::<GetItemInput>().expect("correct type");
    set_table_names(span, i.table_name());
    set_consistent_read(span, i.consistent_read());
    set_projection(span, i.projection_expression());
}

/// Extracts input attributes for the `Query` operation.
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

/// Extracts input attributes for the `Scan` operation.
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

/// Extracts input attributes for the `CreateTable` operation.
fn extract_create_table_input(input: &context::Input, span: &mut impl SpanWrite) {
    let i = input
        .downcast_ref::<CreateTableInput>()
        .expect("correct type");
    set_table_names(span, i.table_name());
    set_provisioned_throughput(span, i.provisioned_throughput());
}

/// Extracts input attributes for the `UpdateTable` operation.
fn extract_update_table_input(input: &context::Input, span: &mut impl SpanWrite) {
    let i = input
        .downcast_ref::<UpdateTableInput>()
        .expect("correct type");
    set_table_names(span, i.table_name());
    set_provisioned_throughput(span, i.provisioned_throughput());
}

/// Extracts input attributes for the `ListTables` operation.
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

/// Extracts input attributes for the `BatchGetItem` operation.
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

/// Extracts input attributes for the `BatchWriteItem` operation.
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

/// Extracts input attributes for the `TransactGetItems` operation.
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

/// Extracts input attributes for the `TransactWriteItems` operation.
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

/// Extracts output attributes for the `Query` operation.
fn extract_query_output(output: &context::Output, span: &mut impl SpanWrite) {
    let o = output.downcast_ref::<QueryOutput>().expect("correct type");
    span.set_attribute(semco::AWS_DYNAMODB_COUNT, Value::I64(i64::from(o.count())));
    span.set_attribute(
        semco::AWS_DYNAMODB_SCANNED_COUNT,
        Value::I64(i64::from(o.scanned_count())),
    );
    set_consumed_capacity_opt(span, o.consumed_capacity());
}

/// Extracts output attributes for the `Scan` operation.
fn extract_scan_output(output: &context::Output, span: &mut impl SpanWrite) {
    let o = output.downcast_ref::<ScanOutput>().expect("correct type");
    span.set_attribute(semco::AWS_DYNAMODB_COUNT, Value::I64(i64::from(o.count())));
    span.set_attribute(
        semco::AWS_DYNAMODB_SCANNED_COUNT,
        Value::I64(i64::from(o.scanned_count())),
    );
    set_consumed_capacity_opt(span, o.consumed_capacity());
}

/// Extracts output attributes for the `ListTables` operation.
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

/// Sets the `aws.dynamodb.table_names` attribute from an iterator of table name strings.
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

/// Sets the `aws.dynamodb.consistent_read` attribute if present.
fn set_consistent_read(span: &mut impl SpanWrite, consistent_read: Option<bool>) {
    if let Some(consistent_read) = consistent_read {
        span.set_attribute(semco::AWS_DYNAMODB_CONSISTENT_READ, consistent_read);
    }
}

/// Sets the `aws.dynamodb.projection` attribute if present.
fn set_projection(span: &mut impl SpanWrite, projection_expression: Option<&str>) {
    if let Some(projection) = projection_expression {
        span.set_attribute(semco::AWS_DYNAMODB_PROJECTION, projection.to_owned());
    }
}

/// Sets the `aws.dynamodb.index_name` attribute if present.
fn set_index_name(span: &mut impl SpanWrite, index_name: Option<&str>) {
    if let Some(index_name) = index_name {
        span.set_attribute(semco::AWS_DYNAMODB_INDEX_NAME, index_name.to_owned());
    }
}

/// Sets the `aws.dynamodb.select` attribute if present.
fn set_select(span: &mut impl SpanWrite, select: Option<&aws_sdk_dynamodb::types::Select>) {
    if let Some(select) = select {
        span.set_attribute(semco::AWS_DYNAMODB_SELECT, select.as_str().to_owned());
    }
}

/// Sets the `aws.dynamodb.limit` attribute if present.
fn set_limit(span: &mut impl SpanWrite, limit: Option<i32>) {
    if let Some(limit) = limit {
        span.set_attribute(semco::AWS_DYNAMODB_LIMIT, Value::I64(i64::from(limit)));
    }
}

/// Sets the `aws.dynamodb.attributes_to_get` attribute if the list is non-empty.
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

/// Sets the `aws.dynamodb.provisioned_read_capacity` and `aws.dynamodb.provisioned_write_capacity` attributes if present.
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

/// Sets the `aws.dynamodb.consumed_capacity` attribute from a single optional `ConsumedCapacity`
/// value (used by `GetItem`, `PutItem`, `DeleteItem`, `UpdateItem`, `Query`, `Scan`).
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

/// Sets the `aws.dynamodb.consumed_capacity` attribute from a list of `ConsumedCapacity` values
/// (used by `BatchGetItem`, `BatchWriteItem`, `TransactGetItems`, `TransactWriteItems`).
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

/// Newtype wrapper for [`types::Capacity`] that implements [`Serialize`].
struct SerCapacity<'a>(&'a types::Capacity);

/// Serializes [`types::Capacity`] as a JSON map with only the present capacity fields.
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

/// Newtype wrapper for [`types::ConsumedCapacity`] that implements [`Serialize`].
struct SerConsumedCapacity<'a>(&'a types::ConsumedCapacity);

/// Serializes [`types::ConsumedCapacity`] as a JSON map matching the OTel semconv format.
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

/// Newtype wrapper for a `HashMap<String, Capacity>` that serializes each value through [`SerCapacity`].
struct SerCapacityMap<'a>(&'a std::collections::HashMap<String, types::Capacity>);

/// Serializes a map of index names to [`types::Capacity`] values.
impl Serialize for SerCapacityMap<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, cap) in self.0 {
            map.serialize_entry(key, &SerCapacity(cap))?;
        }
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use aws_smithy_runtime_api::client::interceptors::context;
    use opentelemetry::Value;

    use crate::span_write::{SpanWrite, Status};

    // ---------------------------------------------------------------------------
    // Test span implementation
    // ---------------------------------------------------------------------------

    struct TestSpan {
        attributes: Vec<(&'static str, Value)>,
        status: Option<Status>,
    }

    impl TestSpan {
        fn new() -> Self {
            Self {
                attributes: vec![],
                status: None,
            }
        }

        fn get(&self, key: &str) -> Option<&Value> {
            self.attributes
                .iter()
                .rev()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v)
        }
    }

    impl SpanWrite for TestSpan {
        fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>) {
            self.attributes.push((key, value.into()));
        }

        fn set_status(&mut self, code: Status) {
            self.status = Some(code);
        }
    }

    // ---------------------------------------------------------------------------
    // SerCapacity serialization — single_comprehensive
    // ---------------------------------------------------------------------------

    #[test]
    fn ser_capacity_all_fields() {
        let cap = types::Capacity::builder()
            .capacity_units(5.0)
            .read_capacity_units(2.0)
            .write_capacity_units(3.0)
            .build();

        let json = serde_json::to_string(&SerCapacity(&cap)).unwrap();

        assert!(json.contains("\"CapacityUnits\":5.0"));
        assert!(json.contains("\"ReadCapacityUnits\":2.0"));
        assert!(json.contains("\"WriteCapacityUnits\":3.0"));
    }

    #[test]
    fn ser_capacity_partial_fields() {
        // Only capacity_units set
        let cap_only_cu = types::Capacity::builder().capacity_units(10.0).build();
        let json = serde_json::to_string(&SerCapacity(&cap_only_cu)).unwrap();
        assert!(json.contains("\"CapacityUnits\":10.0"));
        assert!(!json.contains("ReadCapacityUnits"));
        assert!(!json.contains("WriteCapacityUnits"));

        // Empty capacity
        let cap_empty = types::Capacity::builder().build();
        let json_empty = serde_json::to_string(&SerCapacity(&cap_empty)).unwrap();
        assert_eq!(json_empty, "{}");
    }

    // ---------------------------------------------------------------------------
    // SerConsumedCapacity serialization — single_comprehensive
    // ---------------------------------------------------------------------------

    #[test]
    fn ser_consumed_capacity_all_fields() {
        let table_cap = types::Capacity::builder().capacity_units(1.0).build();
        let cc = types::ConsumedCapacity::builder()
            .table_name("my-table")
            .capacity_units(10.0)
            .read_capacity_units(4.0)
            .write_capacity_units(6.0)
            .table(table_cap)
            .build();

        let json = serde_json::to_string(&SerConsumedCapacity(&cc)).unwrap();

        assert!(json.contains("\"TableName\":\"my-table\""));
        assert!(json.contains("\"CapacityUnits\":10.0"));
        assert!(json.contains("\"ReadCapacityUnits\":4.0"));
        assert!(json.contains("\"WriteCapacityUnits\":6.0"));
        assert!(json.contains("\"Table\":{\"CapacityUnits\":1.0}"));
    }

    #[test]
    fn ser_consumed_capacity_with_indexes() {
        let mut lsi = std::collections::HashMap::new();
        lsi.insert(
            "lsi-1".to_string(),
            types::Capacity::builder().capacity_units(0.5).build(),
        );
        let mut gsi = std::collections::HashMap::new();
        gsi.insert(
            "gsi-1".to_string(),
            types::Capacity::builder().capacity_units(1.5).build(),
        );

        let cc = types::ConsumedCapacity::builder()
            .table_name("my-table")
            .capacity_units(5.0)
            .set_local_secondary_indexes(Some(lsi))
            .set_global_secondary_indexes(Some(gsi))
            .build();

        let json = serde_json::to_string(&SerConsumedCapacity(&cc)).unwrap();

        assert!(json.contains("\"TableName\":\"my-table\""));
        assert!(json.contains("\"LocalSecondaryIndexes\""));
        assert!(json.contains("\"lsi-1\""));
        assert!(json.contains("\"GlobalSecondaryIndexes\""));
        assert!(json.contains("\"gsi-1\""));
    }

    // ---------------------------------------------------------------------------
    // set_table_names — consolidated_2tests
    // ---------------------------------------------------------------------------

    #[test]
    fn set_table_names_non_empty() {
        let mut span = TestSpan::new();

        // Single table
        set_table_names(&mut span, ["orders"]);
        let val = span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES);
        assert!(val.is_some());
        assert!(matches!(val.unwrap(), Value::Array(_)));

        // Multiple tables
        let mut span2 = TestSpan::new();
        set_table_names(&mut span2, ["table-a", "table-b"]);
        let val2 =
            span2.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES);
        assert!(val2.is_some());
        if let Value::Array(opentelemetry::Array::String(names)) = val2.unwrap() {
            assert_eq!(names.len(), 2);
        } else {
            panic!("expected Array::String");
        }
    }

    #[test]
    fn set_table_names_empty() {
        let mut span = TestSpan::new();
        set_table_names(&mut span, std::iter::empty::<&str>());
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES)
                .is_none()
        );
    }

    // ---------------------------------------------------------------------------
    // set_consumed_capacity_opt — consolidated_2tests
    // ---------------------------------------------------------------------------

    #[test]
    fn set_consumed_capacity_opt_some() {
        let cc = types::ConsumedCapacity::builder()
            .table_name("orders")
            .capacity_units(2.0)
            .build();
        let mut span = TestSpan::new();
        set_consumed_capacity_opt(&mut span, Some(&cc));

        let val =
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_CONSUMED_CAPACITY);
        assert!(val.is_some());
        if let Value::Array(opentelemetry::Array::String(items)) = val.unwrap() {
            assert_eq!(items.len(), 1);
            let s: &str = items[0].as_ref();
            assert!(s.contains("orders"));
        } else {
            panic!("expected Array::String");
        }
    }

    #[test]
    fn set_consumed_capacity_opt_none() {
        let mut span = TestSpan::new();
        set_consumed_capacity_opt(&mut span, None);
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_CONSUMED_CAPACITY)
                .is_none()
        );
    }

    // ---------------------------------------------------------------------------
    // set_consumed_capacity_list — consolidated_2tests
    // ---------------------------------------------------------------------------

    #[test]
    fn set_consumed_capacity_list_non_empty() {
        let ccs = vec![
            types::ConsumedCapacity::builder()
                .table_name("table-1")
                .capacity_units(1.0)
                .build(),
            types::ConsumedCapacity::builder()
                .table_name("table-2")
                .capacity_units(2.0)
                .build(),
        ];
        let mut span = TestSpan::new();
        set_consumed_capacity_list(&mut span, &ccs);

        let val =
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_CONSUMED_CAPACITY);
        assert!(val.is_some());
        if let Value::Array(opentelemetry::Array::String(items)) = val.unwrap() {
            assert_eq!(items.len(), 2);
        } else {
            panic!("expected Array::String");
        }
    }

    #[test]
    fn set_consumed_capacity_list_empty() {
        let mut span = TestSpan::new();
        set_consumed_capacity_list(&mut span, &[]);
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_CONSUMED_CAPACITY)
                .is_none()
        );
    }

    // ---------------------------------------------------------------------------
    // extract_get_item_input — consolidated_2tests
    // ---------------------------------------------------------------------------

    #[test]
    fn extract_get_item_input_full() {
        use aws_sdk_dynamodb::operation::get_item::GetItemInput;

        let sdk_input = GetItemInput::builder()
            .table_name("orders")
            .consistent_read(true)
            .projection_expression("id, #name")
            .build()
            .unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();

        extract_get_item_input(&input, &mut span);

        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES)
                .is_some()
        );
        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_CONSISTENT_READ),
            Some(&Value::Bool(true))
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_PROJECTION)
                .is_some()
        );
    }

    #[test]
    fn extract_get_item_input_minimal() {
        use aws_sdk_dynamodb::operation::get_item::GetItemInput;

        let sdk_input = GetItemInput::builder()
            .table_name("orders")
            .build()
            .unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();

        extract_get_item_input(&input, &mut span);

        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES)
                .is_some()
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_CONSISTENT_READ)
                .is_none()
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_PROJECTION)
                .is_none()
        );
    }

    // ---------------------------------------------------------------------------
    // extract_query_input — single_comprehensive
    // ---------------------------------------------------------------------------

    #[test]
    fn extract_query_input_all_attributes() {
        use aws_sdk_dynamodb::operation::query::QueryInput;
        use aws_sdk_dynamodb::types::Select;

        let sdk_input = QueryInput::builder()
            .table_name("orders")
            .consistent_read(true)
            .projection_expression("id, amount")
            .index_name("status-index")
            .select(Select::AllAttributes)
            .limit(50)
            .scan_index_forward(false)
            .attributes_to_get("id")
            .attributes_to_get("amount")
            .build()
            .unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();

        extract_query_input(&input, &mut span);

        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES)
                .is_some()
        );
        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_CONSISTENT_READ),
            Some(&Value::Bool(true))
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_PROJECTION)
                .is_some()
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_INDEX_NAME)
                .is_some()
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_SELECT)
                .is_some()
        );
        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_LIMIT),
            Some(&Value::I64(50))
        );
        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_SCAN_FORWARD),
            Some(&Value::Bool(false))
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_ATTRIBUTES_TO_GET)
                .is_some()
        );
    }

    #[test]
    fn extract_query_input_minimal() {
        use aws_sdk_dynamodb::operation::query::QueryInput;

        let sdk_input = QueryInput::builder().table_name("orders").build().unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();

        extract_query_input(&input, &mut span);

        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES)
                .is_some()
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_SCAN_FORWARD)
                .is_none()
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_LIMIT)
                .is_none()
        );
    }

    // ---------------------------------------------------------------------------
    // extract_scan_input — single_comprehensive
    // ---------------------------------------------------------------------------

    #[test]
    fn extract_scan_input_all_attributes() {
        use aws_sdk_dynamodb::operation::scan::ScanInput;
        use aws_sdk_dynamodb::types::Select;

        let sdk_input = ScanInput::builder()
            .table_name("orders")
            .consistent_read(true)
            .projection_expression("id")
            .index_name("status-index")
            .select(Select::AllAttributes)
            .limit(100)
            .segment(2)
            .total_segments(10)
            .attributes_to_get("id")
            .build()
            .unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();

        extract_scan_input(&input, &mut span);

        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES)
                .is_some()
        );
        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_CONSISTENT_READ),
            Some(&Value::Bool(true))
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_PROJECTION)
                .is_some()
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_INDEX_NAME)
                .is_some()
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_SELECT)
                .is_some()
        );
        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_LIMIT),
            Some(&Value::I64(100))
        );
        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_SEGMENT),
            Some(&Value::I64(2))
        );
        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TOTAL_SEGMENTS),
            Some(&Value::I64(10))
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_ATTRIBUTES_TO_GET)
                .is_some()
        );
    }

    #[test]
    fn extract_scan_input_minimal() {
        use aws_sdk_dynamodb::operation::scan::ScanInput;

        let sdk_input = ScanInput::builder().table_name("orders").build().unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();

        extract_scan_input(&input, &mut span);

        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES)
                .is_some()
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_SEGMENT)
                .is_none()
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TOTAL_SEGMENTS)
                .is_none()
        );
    }

    // ---------------------------------------------------------------------------
    // extract_list_tables_input — consolidated_2tests
    // ---------------------------------------------------------------------------

    #[test]
    fn extract_list_tables_input_with_params() {
        use aws_sdk_dynamodb::operation::list_tables::ListTablesInput;

        let sdk_input = ListTablesInput::builder()
            .limit(20)
            .exclusive_start_table_name("last-seen-table")
            .build()
            .unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();

        extract_list_tables_input(&input, &mut span);

        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_LIMIT),
            Some(&Value::I64(20))
        );
        assert!(
            span.get(
                opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_EXCLUSIVE_START_TABLE
            )
            .is_some()
        );
    }

    #[test]
    fn extract_list_tables_input_empty() {
        use aws_sdk_dynamodb::operation::list_tables::ListTablesInput;

        let sdk_input = ListTablesInput::builder().build().unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();

        extract_list_tables_input(&input, &mut span);

        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_LIMIT)
                .is_none()
        );
        assert!(
            span.get(
                opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_EXCLUSIVE_START_TABLE
            )
            .is_none()
        );
    }

    // ---------------------------------------------------------------------------
    // extract_batch_get_item_input — single_comprehensive
    // ---------------------------------------------------------------------------

    #[test]
    fn extract_batch_get_item_input_table_names() {
        use aws_sdk_dynamodb::operation::batch_get_item::BatchGetItemInput;
        use aws_sdk_dynamodb::types::{AttributeValue, KeysAndAttributes};

        let key_map = {
            let mut m = std::collections::HashMap::new();
            m.insert("pk".to_string(), AttributeValue::S("v".to_string()));
            m
        };
        let keys_and_attrs = KeysAndAttributes::builder()
            .keys(key_map.clone())
            .build()
            .unwrap();
        let keys_and_attrs2 = KeysAndAttributes::builder().keys(key_map).build().unwrap();
        let sdk_input = BatchGetItemInput::builder()
            .request_items("table-alpha", keys_and_attrs)
            .request_items("table-beta", keys_and_attrs2)
            .build()
            .unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();

        extract_batch_get_item_input(&input, &mut span);

        let val = span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES);
        assert!(val.is_some());
        if let Value::Array(opentelemetry::Array::String(names)) = val.unwrap() {
            assert_eq!(names.len(), 2);
            let name_strs: Vec<&str> = names.iter().map(|s| s.as_ref()).collect();
            assert!(name_strs.contains(&"table-alpha"));
            assert!(name_strs.contains(&"table-beta"));
        } else {
            panic!("expected Array::String");
        }
    }

    #[test]
    fn extract_batch_get_item_input_empty() {
        use aws_sdk_dynamodb::operation::batch_get_item::BatchGetItemInput;

        let sdk_input = BatchGetItemInput::builder().build().unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();

        extract_batch_get_item_input(&input, &mut span);

        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES)
                .is_none()
        );
    }

    // ---------------------------------------------------------------------------
    // extract_transact_write_items_input — single_comprehensive
    // ---------------------------------------------------------------------------

    #[test]
    fn extract_transact_write_items_input_all_variants() {
        use aws_sdk_dynamodb::operation::transact_write_items::TransactWriteItemsInput;
        use aws_sdk_dynamodb::types::{
            AttributeValue, ConditionCheck, Delete, Put, TransactWriteItem, Update,
        };

        let key_val = AttributeValue::S("pk-value".to_string());

        let condition_check = ConditionCheck::builder()
            .table_name("table-condition")
            .key("pk", key_val.clone())
            .condition_expression("attribute_exists(pk)")
            .build()
            .unwrap();
        let put = Put::builder()
            .table_name("table-put")
            .item("pk", key_val.clone())
            .build()
            .unwrap();
        let delete = Delete::builder()
            .table_name("table-delete")
            .key("pk", key_val.clone())
            .build()
            .unwrap();
        let update = Update::builder()
            .table_name("table-update")
            .key("pk", key_val)
            .update_expression("SET #s = :s")
            .build()
            .unwrap();

        let sdk_input = TransactWriteItemsInput::builder()
            .transact_items(
                TransactWriteItem::builder()
                    .condition_check(condition_check)
                    .build(),
            )
            .transact_items(TransactWriteItem::builder().put(put).build())
            .transact_items(TransactWriteItem::builder().delete(delete).build())
            .transact_items(TransactWriteItem::builder().update(update).build())
            .build()
            .unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();

        extract_transact_write_items_input(&input, &mut span);

        let val = span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES);
        assert!(val.is_some());
        if let Value::Array(opentelemetry::Array::String(names)) = val.unwrap() {
            // BTreeSet deduplicates and sorts, so we get 4 unique table names
            assert_eq!(names.len(), 4);
            let name_strs: Vec<&str> = names.iter().map(|s| s.as_ref()).collect();
            assert!(name_strs.contains(&"table-condition"));
            assert!(name_strs.contains(&"table-put"));
            assert!(name_strs.contains(&"table-delete"));
            assert!(name_strs.contains(&"table-update"));
        } else {
            panic!("expected Array::String");
        }
    }

    #[test]
    fn extract_transact_write_items_input_empty() {
        use aws_sdk_dynamodb::operation::transact_write_items::TransactWriteItemsInput;

        let sdk_input = TransactWriteItemsInput::builder().build().unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();

        extract_transact_write_items_input(&input, &mut span);

        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES)
                .is_none()
        );
    }

    // ---------------------------------------------------------------------------
    // extract_query_output — single_comprehensive
    // ---------------------------------------------------------------------------

    #[test]
    fn extract_query_output_with_capacity() {
        use aws_sdk_dynamodb::operation::query::QueryOutput;

        let cc = types::ConsumedCapacity::builder()
            .table_name("orders")
            .capacity_units(3.0)
            .build();
        let sdk_output = QueryOutput::builder()
            .count(42)
            .scanned_count(100)
            .consumed_capacity(cc)
            .build();
        let output = context::Output::erase(sdk_output);
        let mut span = TestSpan::new();

        extract_query_output(&output, &mut span);

        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_COUNT),
            Some(&Value::I64(42))
        );
        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_SCANNED_COUNT),
            Some(&Value::I64(100))
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_CONSUMED_CAPACITY)
                .is_some()
        );
    }

    #[test]
    fn extract_query_output_no_capacity() {
        use aws_sdk_dynamodb::operation::query::QueryOutput;

        let sdk_output = QueryOutput::builder().count(5).scanned_count(5).build();
        let output = context::Output::erase(sdk_output);
        let mut span = TestSpan::new();

        extract_query_output(&output, &mut span);

        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_COUNT),
            Some(&Value::I64(5))
        );
        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_SCANNED_COUNT),
            Some(&Value::I64(5))
        );
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_CONSUMED_CAPACITY)
                .is_none()
        );
    }

    // ---------------------------------------------------------------------------
    // extract_list_tables_output — single_comprehensive
    // ---------------------------------------------------------------------------

    #[test]
    fn extract_list_tables_output_count() {
        use aws_sdk_dynamodb::operation::list_tables::ListTablesOutput;

        let sdk_output = ListTablesOutput::builder()
            .table_names("table-a")
            .table_names("table-b")
            .table_names("table-c")
            .build();
        let output = context::Output::erase(sdk_output);
        let mut span = TestSpan::new();

        extract_list_tables_output(&output, &mut span);

        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_COUNT),
            Some(&Value::I64(3))
        );
    }

    #[test]
    fn extract_list_tables_output_empty() {
        use aws_sdk_dynamodb::operation::list_tables::ListTablesOutput;

        let sdk_output = ListTablesOutput::builder().build();
        let output = context::Output::erase(sdk_output);
        let mut span = TestSpan::new();

        extract_list_tables_output(&output, &mut span);

        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_COUNT),
            Some(&Value::I64(0))
        );
    }

    // ---------------------------------------------------------------------------
    // DynamoDBExtractor::extract_input dispatch — consolidated_2tests
    // ---------------------------------------------------------------------------

    #[test]
    fn dynamodb_extractor_extract_input_known_operation() {
        use aws_sdk_dynamodb::operation::put_item::PutItemInput;

        let sdk_input = PutItemInput::builder()
            .table_name("orders")
            .build()
            .unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();
        let extractor = DynamoDBExtractor::new();

        extractor.extract_input("DynamoDB", "PutItem", &input, &mut span);

        // db.system.name is always set
        assert_eq!(
            span.get(crate::interceptor::DB_SYSTEM_NAME),
            Some(&Value::String("aws.dynamodb".into()))
        );
        // table_names is set for PutItem
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES)
                .is_some()
        );
    }

    #[test]
    fn dynamodb_extractor_extract_input_unknown_operation() {
        use aws_sdk_dynamodb::operation::put_item::PutItemInput;

        // For an unknown operation, only db.system.name is set; no downcast happens
        // because the _ arm is taken. We wrap a real SDK type but use an operation
        // name that doesn't match any arm.
        let sdk_input = PutItemInput::builder()
            .table_name("orders")
            .build()
            .unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();
        let extractor = DynamoDBExtractor::new();

        extractor.extract_input("DynamoDB", "UnknownOperation", &input, &mut span);

        // db.system.name is always set
        assert_eq!(
            span.get(crate::interceptor::DB_SYSTEM_NAME),
            Some(&Value::String("aws.dynamodb".into()))
        );
        // No table_names for unknown operation
        assert!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_TABLE_NAMES)
                .is_none()
        );
    }

    // ---------------------------------------------------------------------------
    // DynamoDBExtractor::extract_output dispatch — consolidated_2tests
    // ---------------------------------------------------------------------------

    #[test]
    fn dynamodb_extractor_extract_output_query() {
        use aws_sdk_dynamodb::operation::query::QueryOutput;

        let sdk_output = QueryOutput::builder().count(7).scanned_count(15).build();
        let output = context::Output::erase(sdk_output);
        let mut span = TestSpan::new();
        let extractor = DynamoDBExtractor::new();

        extractor.extract_output("DynamoDB", "Query", &output, &mut span);

        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_COUNT),
            Some(&Value::I64(7))
        );
        assert_eq!(
            span.get(opentelemetry_semantic_conventions::attribute::AWS_DYNAMODB_SCANNED_COUNT),
            Some(&Value::I64(15))
        );
    }

    #[test]
    fn dynamodb_extractor_extract_output_unknown_operation() {
        use aws_sdk_dynamodb::operation::query::QueryOutput;

        // For an unknown operation, the _ arm is taken and no attributes are set.
        // We wrap a real SDK type but use an operation name that doesn't match any arm.
        let sdk_output = QueryOutput::builder().count(99).scanned_count(99).build();
        let output = context::Output::erase(sdk_output);
        let mut span = TestSpan::new();
        let extractor = DynamoDBExtractor::new();

        extractor.extract_output("DynamoDB", "UnknownOperation", &output, &mut span);

        // No attributes set for unknown operation
        assert!(span.attributes.is_empty());
    }
}
