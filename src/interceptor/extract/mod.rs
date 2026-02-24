// Extraction dispatch — Metadata extraction (always available) and
// feature-gated service-specific modules.

#[cfg(feature = "extract-dynamodb")]
pub mod dynamodb;

#[cfg(feature = "extract-s3")]
pub mod s3;

#[cfg(feature = "extract-sqs")]
pub mod sqs;
