//! Lambda support: Tower layer, per-invocation spans, and the
//! `make_lambda_runtime!` macro (`env-lambda` feature).
//!
//! This module re-exports the most commonly used Lambda types and provides
//! the [`make_lambda_runtime!`] macro for zero-boilerplate Lambda setup.
//!
//! ## Sub-modules
//!
//! - [`layer`] — [`layer::TracingLayer`] and [`layer::TracingService`]: Tower
//!   middleware that wraps the Lambda runtime service, creates a span per
//!   invocation, propagates the X-Ray trace context, tracks cold-starts, and
//!   flushes the exporter when the invocation future drops.
//! - [`macros`] — [`make_lambda_runtime!`] and its helper
//!   [`macros::default_flush_tracer`].
//!
//! ## Re-exports
//!
//! - [`lambda_runtime`] — the underlying Lambda runtime crate, available so
//!   that users do not need to add it as a direct dependency.
//! - [`LambdaError`] / [`LambdaEvent`] — convenience aliases for
//!   `lambda_runtime::Error` and `lambda_runtime::LambdaEvent`.
//! - [`OTelFaasTrigger`] — re-exported from [`layer`] for convenience.
//!
//! Note: `tokio` is *not* re-exported. The `#[tokio::main]` attribute used
//! inside [`make_lambda_runtime!`] resolves the `tokio` crate by its absolute
//! path, so users must declare `tokio` as a direct dependency. Rust Lambda
//! functions need `tokio` anyway.
//!
//! ## Quick example
//!
//! ```no_run
//! use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent};
//! use serde_json::Value;
//!
//! async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
//!     Ok(event.payload)
//! }
//!
//! // Generates main(), telemetry init, and the Tower layer.
//! awssdk_instrumentation::make_lambda_runtime!(handler);
//! ```
//!
//! [`make_lambda_runtime!`]: crate::make_lambda_runtime

// Lambda support module — Tower layer and make_lambda_runtime! macro.

pub mod layer;
pub mod macros;

pub use layer::OTelFaasTrigger;

pub use lambda_runtime;

pub use lambda_runtime::{Error as LambdaError, LambdaEvent};
