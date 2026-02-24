// Tracing backend

use opentelemetry::trace::{SpanContext, TraceContextExt, TraceState};
use opentelemetry_semantic_conventions::attribute as semco;

use tracing::Instrument;
use tracing::instrument::Instrumented;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::{InstrumentedFuture, Instrumentor, utils::XRayTraceHeader};

#[derive(Debug, Clone)]
pub struct TracingInstrumentor;

impl<Fut: Future> InstrumentedFuture for Instrumented<Fut> {
    type Fut = Self;
}

impl Instrumentor for TracingInstrumentor {
    type IFut<F: Future> = Instrumented<F>;

    fn instrument<F: Future>(inner: F, context: super::InvocationContext) -> Self::IFut<F> {
        let span = tracing::info_span!(
            "Lambda runtime invoke",
            otel.kind = "server",
            { semco::FAAS_TRIGGER } = context.trigger.to_string(),
            { semco::CLOUD_RESOURCE_ID } = context.function_arn,
            { semco::FAAS_INVOCATION_ID } = context.request_id,
            { semco::CLOUD_ACCOUNT_ID } = context.account_id,
            { semco::FAAS_COLDSTART } = context.is_coldstart
        );

        if let Some(XRayTraceHeader {
            trace_id,
            parent_id,
            sampled,
        }) = context.xray_trace_header
        {
            let otel_context = opentelemetry::Context::new().with_remote_span_context(
                SpanContext::new(trace_id, parent_id, sampled, true, TraceState::NONE),
            );
            span.set_parent(otel_context).expect("not yet activated");
        }

        {
            let _guard = span.enter();
            inner
        }
        .instrument(span)
    }
}
