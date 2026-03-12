use opentelemetry::{Array, StringValue, Value};
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub fn set_xray_annotations_for_current_span(
    fields_list: impl IntoIterator<Item = impl Into<String>>,
) {
    let attr_value = Value::Array(Array::String(
        fields_list
            .into_iter()
            .map(|s| StringValue::from(s.into()))
            .collect(),
    ));
    tracing::Span::current().set_attribute("aws.xray.annotations", attr_value);
}
