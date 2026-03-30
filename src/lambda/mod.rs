//! Lambda support: Tower layer, per-invocation spans, and the
//! `make_lambda_runtime!` macro (`env-lambda` feature).
//!
//! This module re-exports the most commonly used types and provides the
//! [`make_lambda_runtime!`] macro for zero-boilerplate Lambda setup.
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
//! ## Quick example
//!
//! ```no_run
//! use lambda_runtime::{Error, LambdaEvent};
//! use serde_json::Value;
//!
//! async fn handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
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

#[doc(hidden)]
pub use lambda_runtime;
#[doc(hidden)]
pub use opentelemetry_sdk;

pub use layer::OTelFaasTrigger;
