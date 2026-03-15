#[cfg(feature = "tracing-backend")]
mod tracing;

#[cfg(feature = "otel-backend")]
mod otel;

use opentelemetry::{Value, trace::Status};

// Backend-agnostic interface for injecting attributes and status into a span.
pub trait SpanWrite {
    fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>);
    fn set_status(&mut self, code: Status);
}
