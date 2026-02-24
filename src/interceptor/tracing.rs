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

use super::{DefaultExtractor, SpanWrite, utils::StorableOption};

impl SpanWrite for Span {
    fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>) {
        OpenTelemetrySpanExt::set_attribute(self, key, value);
    }

    fn set_status(&mut self, status: Status) {
        OpenTelemetrySpanExt::set_status(self, status);
    }
}

struct PausedSpanGuard {
    paused_spans: Vec<Span>,
}
impl Drop for PausedSpanGuard {
    fn drop(&mut self) {
        // When droping, re-enable the spans in the reverse order of disablement
        while let Some(span) = self.paused_spans.pop() {
            log::trace!("re-enabling span: {span:?}");
            tracing::dispatcher::get_default(|d| d.enter(&span.id().expect("enabled span has id")));
        }
    }
}

struct SpanPauser;

impl SpanPauser {
    fn pause_until<F: Fn(&Span) -> bool>(predicate: F) -> Option<(PausedSpanGuard, Span)> {
        let mut guard = PausedSpanGuard {
            paused_spans: vec![],
        };

        loop {
            // Get the current span
            let span = Span::current();

            // If it is disabled, we consider we cannot go further up
            if span.is_disabled() {
                log::trace!("hit disabled span: {span:?}");
                break;
            }

            // If it matches the predicate, then return it as it is the one we are looking for
            if predicate(&span) {
                log::trace!("span match predicate: {span:?}");
                return Some((guard, span));
            }

            // Else disable the span, store it, and loop around to test the parent.
            log::trace!("disabling span temporarilly: {span:?}");
            tracing::dispatcher::get_default(|d| d.exit(&span.id().expect("enabled span has id")));
            guard.paused_spans.push(span);
        }

        // Re-enable the paused spans if any
        drop(guard);
        None
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
