// S3 attribute extraction — downcasts Input/Output to concrete
// aws-sdk-s3 types and extracts bucket name, key, etc.

use super::super::{AttributeExtractor, SpanWrite};

#[derive(Debug, Default)]
pub struct S3Extractor {
    _private: (),
}

impl S3Extractor {
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl<SW: SpanWrite> AttributeExtractor<SW> for S3Extractor {
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
