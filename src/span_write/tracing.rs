use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::{SpanWrite, Status, Value};

impl SpanWrite for Span {
    fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>) {
        OpenTelemetrySpanExt::set_attribute(self, key, value);
    }

    fn set_status(&mut self, status: Status) {
        OpenTelemetrySpanExt::set_status(self, status);
    }
}
