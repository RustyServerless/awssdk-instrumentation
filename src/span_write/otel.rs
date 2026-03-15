use opentelemetry::{
    Context, KeyValue,
    global::BoxedSpan,
    trace::{Span, TraceContextExt},
};

use super::{SpanWrite, Status, Value};

impl SpanWrite for BoxedSpan {
    fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>) {
        Span::set_attribute(self, KeyValue::new(key, value));
    }

    fn set_status(&mut self, code: Status) {
        Span::set_status(self, code);
    }
}

impl SpanWrite for Context {
    fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>) {
        self.span().set_attribute(KeyValue::new(key, value));
    }

    fn set_status(&mut self, code: Status) {
        self.span().set_status(code);
    }
}
