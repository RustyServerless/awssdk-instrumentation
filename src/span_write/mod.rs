//! Backend-agnostic interface for writing attributes and status into a span.
//!
//! [`SpanWrite`] is the single abstraction that lets the extraction pipeline
//! work identically regardless of whether the active backend is
//! `tracing-backend` or `otel-backend`. Extractors and hooks receive a
//! `&mut impl SpanWrite` and call [`SpanWrite::set_attribute`] or
//! [`SpanWrite::set_status`] without knowing which concrete span type is
//! underneath.
//!
//! The crate provides implementations for:
//!
//! - `tracing::Span` — used by [`crate::interceptor::tracing::TracingInterceptor`]
//!   (`tracing-backend` feature)
//! - `opentelemetry::global::BoxedSpan` — used by
//!   [`crate::interceptor::otel::OtelInterceptor`] (`otel-backend` feature)
//!
//! You only need to interact with this module directly when implementing a
//! custom [`crate::interceptor::AttributeExtractor`].

#[cfg(feature = "tracing-backend")]
mod tracing;

#[cfg(feature = "otel-backend")]
mod otel;

/// Re-export of [`opentelemetry::Value`] for use in [`SpanWrite`] implementations
/// and [`crate::interceptor::AttributeExtractor`] methods.
pub use opentelemetry::{Value, trace::Status};
use opentelemetry_semantic_conventions::trace::HTTP_RESPONSE_STATUS_CODE;

/// Backend-agnostic interface for writing attributes and status into a span.
///
/// `SpanWrite` is the single abstraction that lets the attribute extraction
/// pipeline work identically regardless of whether the active backend is
/// `tracing-backend` or `otel-backend`. Extractors and hooks receive a
/// `&mut impl SpanWrite` and call [`set_attribute`] or [`set_status`] without
/// knowing which concrete span type is underneath.
///
/// The crate provides implementations for:
///
/// - [`::tracing::Span`] — used by
///   [`TracingInterceptor`](`crate::interceptor::tracing::TracingInterceptor`)
///   (`tracing-backend` feature)
/// - [`opentelemetry::global::BoxedSpan`] — used by
///   [`OtelInterceptor`](`crate::interceptor::otel::OtelInterceptor`)
///   (`otel-backend` feature)
///
/// You only need to interact with this trait directly when implementing a
/// custom [`crate::interceptor::AttributeExtractor`].
///
/// # Examples
///
/// Using `SpanWrite` inside a custom [`AttributeExtractor`]:
///
/// ```no_run
/// use awssdk_instrumentation::interceptor::{AttributeExtractor, Operation, Service};
/// use awssdk_instrumentation::span_write::SpanWrite;
/// use aws_smithy_runtime_api::client::interceptors::context;
///
/// struct MyExtractor;
///
/// impl<SW: SpanWrite> AttributeExtractor<SW> for MyExtractor {
///     fn extract_input(
///         &self,
///         _service: Service,
///         _operation: Operation,
///         _input: &context::Input,
///         span: &mut SW,
///     ) {
///         span.set_attribute("app.table", "orders");
///         span.set_http_status_code(200);
///     }
/// }
/// ```
///
/// [`AttributeExtractor`]: crate::interceptor::AttributeExtractor
/// [`set_attribute`]: SpanWrite::set_attribute
/// [`set_status`]: SpanWrite::set_status
pub trait SpanWrite {
    /// Sets a span attribute with the given OTel semantic-convention key and value.
    ///
    /// The `key` must be a `'static` string — use constants from
    /// `opentelemetry_semantic_conventions::attribute` or the crate-level
    /// constants [`DB_SYSTEM_NAME`] and [`RPC_SYSTEM_NAME`].
    ///
    /// The `value` can be any type that implements `Into<`[`Value`]`>`, including
    /// [`String`], [`bool`], [`i64`], [`f64`], and [`Value`] itself.
    ///
    /// [`DB_SYSTEM_NAME`]: crate::interceptor::DB_SYSTEM_NAME
    /// [`RPC_SYSTEM_NAME`]: crate::interceptor::RPC_SYSTEM_NAME
    fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>);

    /// Sets the span status.
    ///
    /// Use [`Status::error`] to mark the span as failed, or [`Status::Ok`] to
    /// mark it as successful. The built-in pipeline calls this automatically
    /// based on the SDK operation result; override it in a custom extractor only
    /// when you need to refine the status.
    fn set_status(&mut self, code: Status);

    /// Sets the `http.response.status_code` attribute from an HTTP status code.
    ///
    /// This is a convenience wrapper around [`set_attribute`] that converts the
    /// [`u16`] status code to the [`i64`] type expected by the OTel semantic
    /// convention.
    ///
    /// [`set_attribute`]: SpanWrite::set_attribute
    fn set_http_status_code(&mut self, code: u16) {
        self.set_attribute(HTTP_RESPONSE_STATUS_CODE, code as i64);
    }
}
