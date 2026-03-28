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

#[derive(Debug, Default)]
pub struct SQSExtractor {
    _private: (),
}

impl SQSExtractor {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

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
