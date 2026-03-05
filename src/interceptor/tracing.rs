// Tracing backend — TracingSpanWriter wrapping a tracing::Span,
// and TracingInterceptor implementing the Intercept trait.

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

use opentelemetry::{Value, trace::Status};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::{
    DefaultExtractor, SpanWrite,
    utils::{SpanPauser, StorableOption},
};

impl SpanWrite for Span {
    fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>) {
        OpenTelemetrySpanExt::set_attribute(self, key, value);
    }

    fn set_status(&mut self, status: Status) {
        OpenTelemetrySpanExt::set_status(self, status);
    }
}

// Intercept implementation using the tracing backend.
#[derive(Debug)]
#[non_exhaustive]
pub struct TracingInterceptor {
    pub extractor: DefaultExtractor<Span>,
}

impl Default for TracingInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

impl TracingInterceptor {
    pub fn new() -> Self {
        Self {
            extractor: DefaultExtractor::new(),
        }
    }
}

impl Intercept for TracingInterceptor {
    fn name(&self) -> &'static str {
        "TracingInterceptor"
    }

    fn read_before_execution(
        &self,
        context: &BeforeSerializationInterceptorContextRef<'_>,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        if let Some((_guard, mut span)) = SpanPauser::pause_until(|span| {
            span.metadata()
                .map(|metadata| metadata.target().contains("::operation::"))
                .unwrap_or_default()
        }) {
            self.extractor
                .read_before_execution(context, cfg, &mut span)?;

            cfg.interceptor_state().store_put(StorableOption::new(span));
        }

        Ok(())
    }

    fn read_after_serialization(
        &self,
        context: &BeforeTransmitInterceptorContextRef<'_>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let mut so_span = std::mem::take(
            cfg.get_mut_from_interceptor_state::<StorableOption<Span>>()
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
            cfg.get_mut_from_interceptor_state::<StorableOption<Span>>()
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
            cfg.get_mut_from_interceptor_state::<StorableOption<Span>>()
                .expect("added in read_before_execution"),
        );

        if let Some(span) = so_span.as_mut() {
            self.extractor.read_after_execution(context, cfg, span)?;
        }

        Ok(())
    }
}
