#[cfg(feature = "tracing-backend")]
mod tracing;

#[cfg(feature = "otel-backend")]
mod otel;

pub use opentelemetry::{Value, trace::Status};
use opentelemetry_semantic_conventions::trace::HTTP_RESPONSE_STATUS_CODE;

// Backend-agnostic interface for injecting attributes and status into a span.
pub trait SpanWrite {
    fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>);
    fn set_status(&mut self, code: Status);
    fn set_http_status_code(&mut self, code: u16) {
        self.set_attribute(HTTP_RESPONSE_STATUS_CODE, code as i64);
    }
}
