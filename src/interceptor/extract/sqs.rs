//! SQS attribute extraction following OTel semantic conventions.
//!
//! This module provides [`SQSExtractor`], which implements
//! [`super::super::AttributeExtractor`] for SQS SDK calls. It is automatically
//! used by [`super::super::DefaultExtractor`] when the `extract-sqs` feature is
//! enabled.
//!
//! ## Extracted attributes
//!
//! Attributes follow the
//! [OTel Messaging semconv](https://opentelemetry.io/docs/specs/semconv/messaging/):
//!
//! **Always set for every SQS operation:**
//! - `messaging.system` = `"aws_sqs"`
//! - `messaging.operation.name` — the SDK operation name (e.g. `"SendMessage"`)
//!
//! **Set when a clear mapping exists:**
//! - `messaging.operation.type` — `"send"` for `SendMessage`/`SendMessageBatch`,
//!   `"receive"` for `ReceiveMessage`, `"settle"` for delete/visibility operations
//!
//! **Set for operations that target a specific queue:**
//! - `aws.sqs.queue.url` — the full queue URL
//! - `messaging.destination.name` — the queue name (last path segment of the URL)
//!
//! **Set from output:**
//! - `messaging.message.id` — for `SendMessage`
//! - `messaging.batch.message_count` — for `SendMessageBatch` and `ReceiveMessage`

// SQS attribute extraction — downcasts Input/Output to concrete
// aws-sdk-sqs types and extracts queue URL, messaging attributes, etc.

use aws_sdk_sqs::operation::{
    add_permission::AddPermissionInput,
    change_message_visibility::ChangeMessageVisibilityInput,
    change_message_visibility_batch::ChangeMessageVisibilityBatchInput,
    delete_message::DeleteMessageInput,
    delete_message_batch::DeleteMessageBatchInput,
    delete_queue::DeleteQueueInput,
    get_queue_attributes::GetQueueAttributesInput,
    list_dead_letter_source_queues::ListDeadLetterSourceQueuesInput,
    list_queue_tags::ListQueueTagsInput,
    purge_queue::PurgeQueueInput,
    receive_message::{ReceiveMessageInput, ReceiveMessageOutput},
    remove_permission::RemovePermissionInput,
    send_message::{SendMessageInput, SendMessageOutput},
    send_message_batch::{SendMessageBatchInput, SendMessageBatchOutput},
    set_queue_attributes::SetQueueAttributesInput,
    tag_queue::TagQueueInput,
    untag_queue::UntagQueueInput,
};
use aws_smithy_runtime_api::client::interceptors::context;
use opentelemetry_semantic_conventions::attribute as semco;

use super::super::{AttributeExtractor, SpanWrite};

/// The well-known `messaging.system` value for Amazon SQS.
const MESSAGING_SYSTEM_VALUE: &str = "aws_sqs";

/// Attribute extractor for SQS SDK calls.
///
/// `SQSExtractor` implements [`AttributeExtractor`] and is automatically used
/// by [`DefaultExtractor`] when the `extract-sqs` feature is enabled. You only
/// need to construct it directly if you are composing a custom extraction
/// pipeline.
///
/// See the [module-level documentation](self) for the full list of extracted
/// attributes.
///
/// [`DefaultExtractor`]: crate::interceptor::DefaultExtractor
#[derive(Debug, Default)]
pub struct SQSExtractor {
    _private: (),
}

impl SQSExtractor {
    /// Creates a new `SQSExtractor`.
    ///
    /// # Examples
    ///
    /// ```
    /// use awssdk_instrumentation::interceptor::extract::sqs::SQSExtractor;
    ///
    /// let extractor = SQSExtractor::new();
    /// ```
    pub fn new() -> Self {
        Self { _private: () }
    }
}

/// Extracts SQS-specific OTel attributes from SDK inputs and outputs.
///
/// See the [module-level documentation](self) for the full list of extracted
/// attributes and which operations they apply to.
impl<SW: SpanWrite> AttributeExtractor<SW> for SQSExtractor {
    fn extract_input(
        &self,
        _service: crate::interceptor::Service,
        operation: crate::interceptor::Operation,
        input: &context::Input,
        span: &mut SW,
    ) {
        // Set messaging.system for all SQS operations
        span.set_attribute(semco::MESSAGING_SYSTEM, MESSAGING_SYSTEM_VALUE);

        // Set messaging.operation.name (the SDK operation name, e.g. "SendMessage")
        span.set_attribute(semco::MESSAGING_OPERATION_NAME, operation.to_owned());

        // Set messaging.operation.type when a clear mapping exists
        if let Some(op_type) = operation_type(operation) {
            span.set_attribute(semco::MESSAGING_OPERATION_TYPE, op_type);
        }

        // Extract and set queue URL + destination name for every operation that has it
        match operation {
            "SendMessage" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<SendMessageInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "SendMessageBatch" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<SendMessageBatchInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "ReceiveMessage" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<ReceiveMessageInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "DeleteMessage" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<DeleteMessageInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "DeleteMessageBatch" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<DeleteMessageBatchInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "ChangeMessageVisibility" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<ChangeMessageVisibilityInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "ChangeMessageVisibilityBatch" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<ChangeMessageVisibilityBatchInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "GetQueueAttributes" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<GetQueueAttributesInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "SetQueueAttributes" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<SetQueueAttributesInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "DeleteQueue" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<DeleteQueueInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "PurgeQueue" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<PurgeQueueInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "ListDeadLetterSourceQueues" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<ListDeadLetterSourceQueuesInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "ListQueueTags" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<ListQueueTagsInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "TagQueue" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<TagQueueInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "UntagQueue" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<UntagQueueInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "AddPermission" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<AddPermissionInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            "RemovePermission" => set_queue_url_attrs(
                span,
                input
                    .downcast_ref::<RemovePermissionInput>()
                    .expect("correct type")
                    .queue_url(),
            ),
            // Operations without queue_url: CreateQueue, GetQueueUrl,
            // ListQueues, CancelMessageMoveTask, ListMessageMoveTasks,
            // StartMessageMoveTask
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
            "SendMessage" => {
                if let Some(message_id) = output
                    .downcast_ref::<SendMessageOutput>()
                    .expect("correct type")
                    .message_id()
                {
                    span.set_attribute(semco::MESSAGING_MESSAGE_ID, message_id.to_owned());
                }
            }
            "SendMessageBatch" => {
                if let Some(output) = output.downcast_ref::<SendMessageBatchOutput>() {
                    let count = output.successful().len();
                    span.set_attribute(semco::MESSAGING_BATCH_MESSAGE_COUNT, count as i64);
                }
            }
            "ReceiveMessage" => {
                if let Some(output) = output.downcast_ref::<ReceiveMessageOutput>() {
                    let count = output.messages().len();
                    span.set_attribute(semco::MESSAGING_BATCH_MESSAGE_COUNT, count as i64);
                }
            }
            _ => {}
        }
    }
}

/// Maps SQS operation names to OTel `messaging.operation.type` values.
///
/// Mapping rationale:
/// - `send`: operations that submit messages to the queue
/// - `receive`: operations that pull messages from the queue
/// - `settle`: operations that acknowledge/delete messages or change their
///   visibility (i.e. message lifecycle settlement)
///
/// Operations that don't map to a messaging operation type (queue management,
/// permissions, tagging, etc.) return `None`.
fn operation_type(operation: &str) -> Option<&'static str> {
    match operation {
        // Producing messages
        "SendMessage" | "SendMessageBatch" => Some("send"),

        // Consuming messages
        "ReceiveMessage" => Some("receive"),

        // Settling messages (delete / change visibility = ack / extend lease)
        "DeleteMessage"
        | "DeleteMessageBatch"
        | "ChangeMessageVisibility"
        | "ChangeMessageVisibilityBatch" => Some("settle"),

        // Queue management, permissions, tagging, etc. — no messaging operation type
        _ => None,
    }
}

/// Sets `aws.sqs.queue.url` and `messaging.destination.name` (the queue name
/// extracted from the last path segment of the URL).
fn set_queue_url_attrs(span: &mut impl SpanWrite, queue_url: Option<&str>) {
    if let Some(url) = queue_url {
        span.set_attribute(semco::AWS_SQS_QUEUE_URL, url.to_owned());

        // SQS queue URLs follow the pattern:
        //   https://sqs.<region>.amazonaws.com/<account-id>/<queue-name>
        // The queue name is the last path segment.
        if let Some(queue_name) = url.rsplit('/').next().filter(|s| !s.is_empty()) {
            span.set_attribute(semco::MESSAGING_DESTINATION_NAME, queue_name.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::Value;
    use opentelemetry_semantic_conventions::attribute as semco;

    use crate::span_write::{SpanWrite, Status};

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

    // Tests for operation_type — 2 consolidated tests

    #[test]
    fn operation_type_known_mappings() {
        // send operations
        assert_eq!(operation_type("SendMessage"), Some("send"));
        assert_eq!(operation_type("SendMessageBatch"), Some("send"));

        // receive operations
        assert_eq!(operation_type("ReceiveMessage"), Some("receive"));

        // settle operations
        assert_eq!(operation_type("DeleteMessage"), Some("settle"));
        assert_eq!(operation_type("DeleteMessageBatch"), Some("settle"));
        assert_eq!(operation_type("ChangeMessageVisibility"), Some("settle"));
        assert_eq!(
            operation_type("ChangeMessageVisibilityBatch"),
            Some("settle")
        );
    }

    #[test]
    fn operation_type_no_mapping() {
        // Queue management operations have no messaging operation type
        assert_eq!(operation_type("CreateQueue"), None);
        assert_eq!(operation_type("GetQueueUrl"), None);
        assert_eq!(operation_type("ListQueues"), None);
        assert_eq!(operation_type("PurgeQueue"), None);
        assert_eq!(operation_type("UnknownOp"), None);
    }

    // Tests for SQSExtractor::extract_output — 2 consolidated tests

    #[test]
    fn extract_output_valid_outputs() {
        use aws_sdk_sqs::operation::{
            receive_message::ReceiveMessageOutput, send_message::SendMessageOutput,
            send_message_batch::SendMessageBatchOutput,
        };
        use aws_sdk_sqs::types::{Message, SendMessageBatchResultEntry};
        use aws_smithy_runtime_api::client::interceptors::context;

        let extractor = SQSExtractor::new();

        // SendMessage with a message_id set — should set messaging.message.id
        let sdk_output = SendMessageOutput::builder()
            .message_id("msg-abc-123")
            .build();
        let output = context::Output::erase(sdk_output);
        let mut span = TestSpan::new();
        extractor.extract_output("SQS", "SendMessage", &output, &mut span);
        assert_eq!(
            span.get(semco::MESSAGING_MESSAGE_ID),
            Some(&Value::from("msg-abc-123"))
        );

        // SendMessageBatch with 2 successful entries — should set messaging.batch.message_count = 2
        let entry1 = SendMessageBatchResultEntry::builder()
            .id("id-1")
            .message_id("msg-1")
            .md5_of_message_body("abc")
            .build()
            .unwrap();
        let entry2 = SendMessageBatchResultEntry::builder()
            .id("id-2")
            .message_id("msg-2")
            .md5_of_message_body("def")
            .build()
            .unwrap();
        let sdk_output = SendMessageBatchOutput::builder()
            .successful(entry1)
            .successful(entry2)
            .set_failed(Some(vec![]))
            .build()
            .unwrap();
        let output = context::Output::erase(sdk_output);
        let mut span = TestSpan::new();
        extractor.extract_output("SQS", "SendMessageBatch", &output, &mut span);
        assert_eq!(
            span.get(semco::MESSAGING_BATCH_MESSAGE_COUNT),
            Some(&Value::I64(2))
        );

        // ReceiveMessage with 3 messages — should set messaging.batch.message_count = 3
        let sdk_output = ReceiveMessageOutput::builder()
            .messages(Message::builder().message_id("m1").build())
            .messages(Message::builder().message_id("m2").build())
            .messages(Message::builder().message_id("m3").build())
            .build();
        let output = context::Output::erase(sdk_output);
        let mut span = TestSpan::new();
        extractor.extract_output("SQS", "ReceiveMessage", &output, &mut span);
        assert_eq!(
            span.get(semco::MESSAGING_BATCH_MESSAGE_COUNT),
            Some(&Value::I64(3))
        );
    }

    #[test]
    fn extract_output_noop_and_edge_cases() {
        use aws_sdk_sqs::operation::{
            receive_message::ReceiveMessageOutput, send_message::SendMessageOutput,
            send_message_batch::SendMessageBatchOutput,
        };
        use aws_smithy_runtime_api::client::interceptors::context;

        let extractor = SQSExtractor::new();

        // SendMessage with no message_id — should NOT set messaging.message.id
        let sdk_output = SendMessageOutput::builder().build();
        let output = context::Output::erase(sdk_output);
        let mut span = TestSpan::new();
        extractor.extract_output("SQS", "SendMessage", &output, &mut span);
        assert!(span.get(semco::MESSAGING_MESSAGE_ID).is_none());

        // Unknown operation — should set no attributes at all
        let sdk_output = SendMessageOutput::builder().build();
        let output = context::Output::erase(sdk_output);
        let mut span = TestSpan::new();
        extractor.extract_output("SQS", "UnknownOperation", &output, &mut span);
        assert!(span.attributes.is_empty());

        // SendMessageBatch with no successful entries — should set count = 0
        let sdk_output = SendMessageBatchOutput::builder()
            .set_successful(Some(vec![]))
            .set_failed(Some(vec![]))
            .build()
            .unwrap();
        let output = context::Output::erase(sdk_output);
        let mut span = TestSpan::new();
        extractor.extract_output("SQS", "SendMessageBatch", &output, &mut span);
        assert_eq!(
            span.get(semco::MESSAGING_BATCH_MESSAGE_COUNT),
            Some(&Value::I64(0))
        );

        // ReceiveMessage with no messages — should set count = 0
        let sdk_output = ReceiveMessageOutput::builder().build();
        let output = context::Output::erase(sdk_output);
        let mut span = TestSpan::new();
        extractor.extract_output("SQS", "ReceiveMessage", &output, &mut span);
        assert_eq!(
            span.get(semco::MESSAGING_BATCH_MESSAGE_COUNT),
            Some(&Value::I64(0))
        );
    }

    // Tests for set_queue_url_attrs — 2 consolidated tests

    #[test]
    fn set_queue_url_attrs_valid_url() {
        let mut span = TestSpan::new();
        set_queue_url_attrs(
            &mut span,
            Some("https://sqs.us-east-1.amazonaws.com/123456789012/my-queue"),
        );

        assert_eq!(
            span.get(semco::AWS_SQS_QUEUE_URL),
            Some(&Value::from(
                "https://sqs.us-east-1.amazonaws.com/123456789012/my-queue"
            ))
        );
        assert_eq!(
            span.get(semco::MESSAGING_DESTINATION_NAME),
            Some(&Value::from("my-queue"))
        );
    }

    #[test]
    fn set_queue_url_attrs_none_and_trailing_slash() {
        // None URL: no attributes set
        let mut span = TestSpan::new();
        set_queue_url_attrs(&mut span, None);
        assert!(span.attributes.is_empty());

        // URL with trailing slash: destination name should not be set (empty segment filtered out)
        let mut span = TestSpan::new();
        set_queue_url_attrs(
            &mut span,
            Some("https://sqs.us-east-1.amazonaws.com/123456789012/my-queue/"),
        );
        assert_eq!(
            span.get(semco::AWS_SQS_QUEUE_URL),
            Some(&Value::from(
                "https://sqs.us-east-1.amazonaws.com/123456789012/my-queue/"
            ))
        );
        // Trailing slash produces an empty last segment, which is filtered out
        assert_eq!(span.get(semco::MESSAGING_DESTINATION_NAME), None);
    }
}
