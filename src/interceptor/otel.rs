// OTel-native backend — OtelSpanWriter wrapping an opentelemetry::trace::Span,
// and OtelInterceptor implementing the Intercept trait.

use aws_smithy_runtime_api::{
    box_error::BoxError,
    client::{
        interceptors::{
            Intercept,
            context::{
                BeforeDeserializationInterceptorContextRef,
                BeforeSerializationInterceptorContextRef, BeforeTransmitInterceptorContextRef,
                FinalizerInterceptorContextRef,
            },
        },
        runtime_components::RuntimeComponents,
    },
};
use aws_smithy_types::config_bag::ConfigBag;

use opentelemetry::{
    KeyValue, Value,
    global::BoxedSpan,
    trace::{Span as SpanTrait, SpanBuilder, SpanKind, Status, Tracer},
};
use opentelemetry_semantic_conventions::attribute as semco;

use super::{
    DefaultExtractor, SpanWrite,
    utils::{StorableOption, extract_service_operation},
};

impl SpanWrite for BoxedSpan {
    fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>) {
        SpanTrait::set_attribute(self, KeyValue::new(key, value));
    }

    fn set_status(&mut self, code: Status) {
        SpanTrait::set_status(self, code);
    }
}

// Intercept implementation using the OTel-native backend.
#[derive(Debug)]
#[non_exhaustive]
pub struct OtelInterceptor {
    pub extractor: DefaultExtractor<BoxedSpan>,
}

impl Default for OtelInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

impl OtelInterceptor {
    pub fn new() -> Self {
        Self {
            extractor: DefaultExtractor::new(),
        }
    }
}

impl Intercept for OtelInterceptor {
    fn name(&self) -> &'static str {
        "OtelInterceptor"
    }

    fn read_before_execution(
        &self,
        context: &BeforeSerializationInterceptorContextRef<'_>,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let start_time = opentelemetry::time::now();
        let (service, operation) = extract_service_operation(cfg);
        let mut span = opentelemetry::global::tracer("").build(
            SpanBuilder::from_name(format!("{service}.{operation}"))
                .with_start_time(start_time)
                .with_kind(SpanKind::Client)
                .with_attributes([
                    KeyValue::new(semco::RPC_SYSTEM, "aws-api"),
                    KeyValue::new(semco::RPC_SERVICE, service.to_owned()),
                    KeyValue::new(semco::RPC_METHOD, operation.to_owned()),
                ]),
        );

        self.extractor
            .read_before_execution(context, cfg, &mut span)?;

        cfg.interceptor_state().store_put(StorableOption::new(span));
        Ok(())
    }

    fn read_after_serialization(
        &self,
        context: &BeforeTransmitInterceptorContextRef<'_>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let mut so_span = std::mem::take(
            cfg.get_mut_from_interceptor_state::<StorableOption<BoxedSpan>>()
                .expect("added in read_before_execution"),
        );
        if let Some(span) = so_span.as_mut() {
            self.extractor
                .read_after_serialization(context, cfg, span)?;
        }
        cfg.interceptor_state().store_put(so_span);
        Ok(())
    }

    fn read_before_deserialization(
        &self,
        context: &BeforeDeserializationInterceptorContextRef<'_>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let mut so_span = std::mem::take(
            cfg.get_mut_from_interceptor_state::<StorableOption<BoxedSpan>>()
                .expect("added in read_before_execution"),
        );
        if let Some(span) = so_span.as_mut() {
            self.extractor
                .read_before_deserialization(context, cfg, span)?;
        }
        cfg.interceptor_state().store_put(so_span);
        Ok(())
    }

    fn read_after_execution(
        &self,
        context: &FinalizerInterceptorContextRef<'_>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let mut so_span = std::mem::take(
            cfg.get_mut_from_interceptor_state::<StorableOption<BoxedSpan>>()
                .expect("added in read_before_execution"),
        );

        if let Some(span) = so_span.as_mut() {
            self.extractor.read_after_execution(context, cfg, span)?;
        }

        Ok(())
    }
}
