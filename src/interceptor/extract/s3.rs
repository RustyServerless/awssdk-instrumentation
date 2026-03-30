//! S3 attribute extraction following OTel semantic conventions.
//!
//! This module provides [`S3Extractor`], which implements
//! [`super::super::AttributeExtractor`] for S3 SDK calls. It is automatically
//! used by [`super::super::DefaultExtractor`] when the `extract-s3` feature is
//! enabled.
//!
//! ## Extracted attributes
//!
//! Attributes follow the
//! [OTel S3 semconv](https://opentelemetry.io/docs/specs/semconv/object-stores/s3/):
//!
//! - `aws.s3.bucket` — set for all operations that target a specific bucket
//! - `aws.s3.key` — set for object-level operations (`GetObject`, `PutObject`,
//!   `DeleteObject`, `HeadObject`, `CopyObject`, multipart upload operations, …)
//! - `aws.s3.copy_source` — set for `CopyObject` and `UploadPartCopy`
//! - `aws.s3.upload_id` — set for multipart upload operations
//! - `aws.s3.part_number` — set for `GetObject`, `HeadObject`, `UploadPart`,
//!   `UploadPartCopy`

// S3 attribute extraction — downcasts Input/Output to concrete
// aws-sdk-s3 types and extracts bucket name, key, etc.

use aws_sdk_s3::operation::{
    abort_multipart_upload::AbortMultipartUploadInput,
    complete_multipart_upload::CompleteMultipartUploadInput, copy_object::CopyObjectInput,
    create_bucket::CreateBucketInput, create_multipart_upload::CreateMultipartUploadInput,
    delete_bucket::DeleteBucketInput, delete_object::DeleteObjectInput,
    delete_objects::DeleteObjectsInput, get_bucket_location::GetBucketLocationInput,
    get_bucket_policy::GetBucketPolicyInput, get_object::GetObjectInput,
    head_bucket::HeadBucketInput, head_object::HeadObjectInput, list_objects::ListObjectsInput,
    list_objects_v2::ListObjectsV2Input, list_parts::ListPartsInput,
    put_bucket_lifecycle_configuration::PutBucketLifecycleConfigurationInput,
    put_object::PutObjectInput, restore_object::RestoreObjectInput,
    select_object_content::SelectObjectContentInput, upload_part::UploadPartInput,
    upload_part_copy::UploadPartCopyInput,
};
use aws_smithy_runtime_api::client::interceptors::context;
use opentelemetry::Value;
use opentelemetry_semantic_conventions::attribute as semco;

use super::super::{AttributeExtractor, SpanWrite};

/// Attribute extractor for S3 SDK calls.
///
/// `S3Extractor` implements [`AttributeExtractor`] and is automatically used by
/// [`DefaultExtractor`] when the `extract-s3` feature is enabled. You only need
/// to construct it directly if you are composing a custom extraction pipeline.
///
/// See the [module-level documentation](self) for the full list of extracted
/// attributes.
///
/// [`DefaultExtractor`]: crate::interceptor::DefaultExtractor
#[derive(Debug, Default)]
pub struct S3Extractor {
    _private: (),
}

impl S3Extractor {
    /// Creates a new `S3Extractor`.
    ///
    /// # Examples
    ///
    /// ```
    /// use awssdk_instrumentation::interceptor::extract::s3::S3Extractor;
    ///
    /// let extractor = S3Extractor::new();
    /// ```
    pub fn new() -> Self {
        Self { _private: () }
    }
}

/// Extracts S3-specific OTel attributes from SDK inputs.
///
/// See the [module-level documentation](self) for the full list of extracted
/// attributes and which operations they apply to.
impl<SW: SpanWrite> AttributeExtractor<SW> for S3Extractor {
    fn extract_input(
        &self,
        _service: crate::interceptor::Service,
        operation: crate::interceptor::Operation,
        input: &context::Input,
        span: &mut SW,
    ) {
        match operation {
            "GetObject" => {
                let i = input
                    .downcast_ref::<GetObjectInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
                set_key(span, i.key());
                set_part_number(span, i.part_number());
            }
            "PutObject" => {
                let i = input
                    .downcast_ref::<PutObjectInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
                set_key(span, i.key());
            }
            "DeleteObject" => {
                let i = input
                    .downcast_ref::<DeleteObjectInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
                set_key(span, i.key());
            }
            "DeleteObjects" => {
                // TODO: The OTel semconv defines `aws.s3.delete` for the delete request
                // container (the `--delete` parameter). The SDK exposes this as a structured
                // `Delete` type, not a string. Serialising it to the expected string format
                // (e.g. "Objects=[{Key=string,VersionId=string}],Quiet=boolean") is non-trivial
                // and low-value. Revisit if users request it.
                let i = input
                    .downcast_ref::<DeleteObjectsInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
            }
            "HeadObject" => {
                let i = input
                    .downcast_ref::<HeadObjectInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
                set_key(span, i.key());
                set_part_number(span, i.part_number());
            }
            "CopyObject" => {
                let i = input
                    .downcast_ref::<CopyObjectInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
                set_key(span, i.key());
                set_copy_source(span, i.copy_source());
            }
            "CreateMultipartUpload" => {
                let i = input
                    .downcast_ref::<CreateMultipartUploadInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
                set_key(span, i.key());
            }
            "CompleteMultipartUpload" => {
                let i = input
                    .downcast_ref::<CompleteMultipartUploadInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
                set_key(span, i.key());
                set_upload_id(span, i.upload_id());
            }
            "AbortMultipartUpload" => {
                let i = input
                    .downcast_ref::<AbortMultipartUploadInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
                set_key(span, i.key());
                set_upload_id(span, i.upload_id());
            }
            "UploadPart" => {
                let i = input
                    .downcast_ref::<UploadPartInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
                set_key(span, i.key());
                set_upload_id(span, i.upload_id());
                set_part_number(span, i.part_number());
            }
            "UploadPartCopy" => {
                let i = input
                    .downcast_ref::<UploadPartCopyInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
                set_key(span, i.key());
                set_copy_source(span, i.copy_source());
                set_upload_id(span, i.upload_id());
                set_part_number(span, i.part_number());
            }
            "ListParts" => {
                let i = input
                    .downcast_ref::<ListPartsInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
                set_key(span, i.key());
                set_upload_id(span, i.upload_id());
            }
            "ListObjectsV2" => {
                let i = input
                    .downcast_ref::<ListObjectsV2Input>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
            }
            "ListObjects" => {
                let i = input
                    .downcast_ref::<ListObjectsInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
            }
            "HeadBucket" => {
                let i = input
                    .downcast_ref::<HeadBucketInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
            }
            "CreateBucket" => {
                let i = input
                    .downcast_ref::<CreateBucketInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
            }
            "DeleteBucket" => {
                let i = input
                    .downcast_ref::<DeleteBucketInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
            }
            "GetBucketLocation" => {
                let i = input
                    .downcast_ref::<GetBucketLocationInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
            }
            "PutBucketLifecycleConfiguration" => {
                let i = input
                    .downcast_ref::<PutBucketLifecycleConfigurationInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
            }
            "GetBucketPolicy" => {
                let i = input
                    .downcast_ref::<GetBucketPolicyInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
            }
            "RestoreObject" => {
                let i = input
                    .downcast_ref::<RestoreObjectInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
                set_key(span, i.key());
            }
            "SelectObjectContent" => {
                let i = input
                    .downcast_ref::<SelectObjectContentInput>()
                    .expect("correct type");
                set_bucket(span, i.bucket());
                set_key(span, i.key());
            }
            // Do nothing for other operations
            _ => {}
        };
    }
}

/// Sets the `aws.s3.bucket` attribute if present.
fn set_bucket(span: &mut impl SpanWrite, bucket: Option<&str>) {
    if let Some(bucket) = bucket {
        span.set_attribute(semco::AWS_S3_BUCKET, bucket.to_owned());
    }
}

/// Sets the `aws.s3.key` attribute if present.
fn set_key(span: &mut impl SpanWrite, key: Option<&str>) {
    if let Some(key) = key {
        span.set_attribute(semco::AWS_S3_KEY, key.to_owned());
    }
}

/// Sets the `aws.s3.copy_source` attribute if present.
fn set_copy_source(span: &mut impl SpanWrite, copy_source: Option<&str>) {
    if let Some(copy_source) = copy_source {
        span.set_attribute(semco::AWS_S3_COPY_SOURCE, copy_source.to_owned());
    }
}

/// Sets the `aws.s3.upload_id` attribute if present.
fn set_upload_id(span: &mut impl SpanWrite, upload_id: Option<&str>) {
    if let Some(upload_id) = upload_id {
        span.set_attribute(semco::AWS_S3_UPLOAD_ID, upload_id.to_owned());
    }
}

/// Sets the `aws.s3.part_number` attribute if present.
fn set_part_number(span: &mut impl SpanWrite, part_number: Option<i32>) {
    if let Some(part_number) = part_number {
        span.set_attribute(
            semco::AWS_S3_PART_NUMBER,
            Value::I64(i64::from(part_number)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_s3::operation::{copy_object::CopyObjectInput, get_object::GetObjectInput};
    use aws_smithy_runtime_api::client::interceptors::context;
    use opentelemetry::Value;
    use opentelemetry_semantic_conventions::attribute as semco;

    use crate::span_write::{SpanWrite, Status};

    // ── TestSpan helper ──────────────────────────────────────────────────────

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

    // ── set_bucket ───────────────────────────────────────────────────────────

    #[test]
    fn set_bucket_with_value() {
        let mut span = TestSpan::new();
        set_bucket(&mut span, Some("my-bucket"));
        assert_eq!(
            span.get(semco::AWS_S3_BUCKET),
            Some(&Value::String("my-bucket".into()))
        );
    }

    #[test]
    fn set_bucket_none() {
        let mut span = TestSpan::new();
        set_bucket(&mut span, None);
        assert!(span.get(semco::AWS_S3_BUCKET).is_none());
    }

    // ── set_key ──────────────────────────────────────────────────────────────

    #[test]
    fn set_key_with_value() {
        let mut span = TestSpan::new();
        set_key(&mut span, Some("path/to/object.txt"));
        assert_eq!(
            span.get(semco::AWS_S3_KEY),
            Some(&Value::String("path/to/object.txt".into()))
        );
    }

    #[test]
    fn set_key_none() {
        let mut span = TestSpan::new();
        set_key(&mut span, None);
        assert!(span.get(semco::AWS_S3_KEY).is_none());
    }

    // ── set_copy_source ──────────────────────────────────────────────────────

    #[test]
    fn set_copy_source_with_value() {
        let mut span = TestSpan::new();
        set_copy_source(&mut span, Some("source-bucket/source-key"));
        assert_eq!(
            span.get(semco::AWS_S3_COPY_SOURCE),
            Some(&Value::String("source-bucket/source-key".into()))
        );
    }

    #[test]
    fn set_copy_source_none() {
        let mut span = TestSpan::new();
        set_copy_source(&mut span, None);
        assert!(span.get(semco::AWS_S3_COPY_SOURCE).is_none());
    }

    // ── set_upload_id ────────────────────────────────────────────────────────

    #[test]
    fn set_upload_id_with_value() {
        let mut span = TestSpan::new();
        set_upload_id(&mut span, Some("upload-123"));
        assert_eq!(
            span.get(semco::AWS_S3_UPLOAD_ID),
            Some(&Value::String("upload-123".into()))
        );
    }

    #[test]
    fn set_upload_id_none() {
        let mut span = TestSpan::new();
        set_upload_id(&mut span, None);
        assert!(span.get(semco::AWS_S3_UPLOAD_ID).is_none());
    }

    // ── set_part_number ──────────────────────────────────────────────────────

    #[test]
    fn set_part_number_with_value() {
        let mut span = TestSpan::new();
        set_part_number(&mut span, Some(42_i32));
        assert_eq!(span.get(semco::AWS_S3_PART_NUMBER), Some(&Value::I64(42)));
    }

    #[test]
    fn set_part_number_none() {
        let mut span = TestSpan::new();
        set_part_number(&mut span, None);
        assert!(span.get(semco::AWS_S3_PART_NUMBER).is_none());
    }

    // ── S3Extractor::extract_input dispatch ──────────────────────────────────

    #[test]
    fn extract_input_get_object_sets_bucket_key_part_number() {
        let extractor = S3Extractor::new();

        // GetObject — sets bucket, key, part_number
        let sdk_input = GetObjectInput::builder()
            .bucket("my-bucket")
            .key("path/to/object.txt")
            .part_number(7)
            .build()
            .unwrap();
        let input = context::Input::erase(sdk_input);
        let mut span = TestSpan::new();
        extractor.extract_input("S3", "GetObject", &input, &mut span);

        assert_eq!(
            span.get(semco::AWS_S3_BUCKET),
            Some(&Value::String("my-bucket".into()))
        );
        assert_eq!(
            span.get(semco::AWS_S3_KEY),
            Some(&Value::String("path/to/object.txt".into()))
        );
        assert_eq!(span.get(semco::AWS_S3_PART_NUMBER), Some(&Value::I64(7)));

        // GetObject without part_number — part_number attribute must be absent
        let sdk_input_no_part = GetObjectInput::builder()
            .bucket("other-bucket")
            .key("other/key")
            .build()
            .unwrap();
        let input_no_part = context::Input::erase(sdk_input_no_part);
        let mut span2 = TestSpan::new();
        extractor.extract_input("S3", "GetObject", &input_no_part, &mut span2);
        assert!(span2.get(semco::AWS_S3_PART_NUMBER).is_none());
    }

    #[test]
    fn extract_input_copy_object_and_unknown_operation() {
        let extractor = S3Extractor::new();

        // CopyObject — sets bucket, key, copy_source
        let copy_input = CopyObjectInput::builder()
            .bucket("dest-bucket")
            .key("dest/key")
            .copy_source("source-bucket/source-key")
            .build()
            .unwrap();
        let input = context::Input::erase(copy_input);
        let mut span = TestSpan::new();
        extractor.extract_input("S3", "CopyObject", &input, &mut span);

        assert_eq!(
            span.get(semco::AWS_S3_BUCKET),
            Some(&Value::String("dest-bucket".into()))
        );
        assert_eq!(
            span.get(semco::AWS_S3_KEY),
            Some(&Value::String("dest/key".into()))
        );
        assert_eq!(
            span.get(semco::AWS_S3_COPY_SOURCE),
            Some(&Value::String("source-bucket/source-key".into()))
        );
        // upload_id must not be set for CopyObject
        assert!(span.get(semco::AWS_S3_UPLOAD_ID).is_none());

        // Unknown operation — no attributes set
        let get_input = GetObjectInput::builder()
            .bucket("my-bucket")
            .key("some/key")
            .build()
            .unwrap();
        let input_unknown = context::Input::erase(get_input);
        let mut span_unknown = TestSpan::new();
        extractor.extract_input("S3", "UnknownOperation", &input_unknown, &mut span_unknown);
        assert!(span_unknown.attributes.is_empty());
    }
}
