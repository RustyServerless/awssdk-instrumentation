//! [`SpanWrite`] implementations for the OTel-native backend.

use opentelemetry::{
    Context, KeyValue,
    global::BoxedSpan,
    trace::{Span, TraceContextExt},
};

use super::{SpanWrite, Status, Value};

/// [`SpanWrite`] impl for OTel's [`BoxedSpan`], setting attributes and status directly on the span.
impl SpanWrite for BoxedSpan {
    fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>) {
        Span::set_attribute(self, KeyValue::new(key, value));
    }

    fn set_status(&mut self, code: Status) {
        Span::set_status(self, code);
    }
}

/// [`SpanWrite`] impl for OTel [`Context`], forwarding attribute and status writes to the active span.
impl SpanWrite for Context {
    fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>) {
        self.span().set_attribute(KeyValue::new(key, value));
    }

    fn set_status(&mut self, code: Status) {
        self.span().set_status(code);
    }
}
