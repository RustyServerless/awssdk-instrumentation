// SQS attribute extraction — downcasts Input/Output to concrete
// aws-sdk-sqs types and extracts queue URL, message attributes, etc.

use super::super::{AttributeExtractor, SpanWrite};

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
        _operation: crate::interceptor::Operation,
        _input: &aws_smithy_runtime_api::client::interceptors::context::Input,
        _span: &mut SW,
    ) {
    }

    fn extract_output(
        &self,
        _service: crate::interceptor::Service,
        _operation: crate::interceptor::Operation,
        _output: &aws_smithy_runtime_api::client::interceptors::context::Output,
        _span: &mut SW,
    ) {
    }
}
