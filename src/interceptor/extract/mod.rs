//! Feature-gated per-service attribute extractors.
//!
//! Each sub-module implements [`super::AttributeExtractor`] for a specific AWS
//! service and is compiled only when the corresponding feature flag is enabled.
//! The [`super::DefaultExtractor`] dispatches to these modules automatically —
//! you do not need to reference them directly unless you want to instantiate an
//! extractor on its own.
//!
//! | Sub-module                  | Feature            | Service   |
//! |-----------------------------|--------------------|-----------|
//! | [`dynamodb`]                | `extract-dynamodb` | DynamoDB  |
//! | [`s3`]                      | `extract-s3`       | S3        |
//! | [`sqs`]                     | `extract-sqs`      | SQS       |

// Extraction dispatch — Metadata extraction (always available) and
// feature-gated service-specific modules.

#[cfg(feature = "extract-dynamodb")]
pub mod dynamodb;

#[cfg(feature = "extract-s3")]
pub mod s3;

#[cfg(feature = "extract-sqs")]
pub mod sqs;
