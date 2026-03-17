// Tracing backend

use opentelemetry::trace::{SpanContext, TraceContextExt, TraceState};
use opentelemetry_semantic_conventions::attribute as semco;

use tokio::task::futures::TaskLocalFuture;
use tracing::instrument::Instrumented;
use tracing::{Instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::{InstrumentedFuture, Instrumentor, utils::XRayTraceHeader};

#[derive(Debug, Clone)]
pub struct TracingInstrumentor;

impl<Fut: Future> InstrumentedFuture for Instrumented<Fut> {
    type Fut = Self;
}

tokio::task_local! {
    static INVOCATION_SPAN: Span;
}

impl Instrumentor for TracingInstrumentor {
    type IFut<F: Future> = Instrumented<TaskLocalFuture<Span, F>>;
    type InvocationSpan = Span;

    fn instrument<F: Future>(inner: F, context: super::InvocationContext) -> Self::IFut<F> {
        let span = tracing::info_span!(
            "Lambda runtime invoke",
            otel.kind = "server",
            { semco::FAAS_TRIGGER } = context.trigger.to_string(),
            { semco::CLOUD_RESOURCE_ID } = context.function_arn,
            { semco::FAAS_INVOCATION_ID } = context.request_id,
            { semco::CLOUD_ACCOUNT_ID } = context.account_id,
            { semco::FAAS_COLDSTART } = context.is_coldstart,
            xray_trace_id = tracing::field::Empty,
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
            span.record("xray_trace_id", trace_id.to_string());
            span.set_parent(otel_context).expect("not yet activated");
        }

        // Scope the task-local so with_invocation_span can find it
        let inner = INVOCATION_SPAN.scope(span.clone(), inner);

        {
            let _guard = span.enter();
            inner
        }
        .instrument(span)
    }

    fn with_invocation_span(f: impl FnOnce(&mut Self::InvocationSpan)) {
        INVOCATION_SPAN.with(|span| {
            let mut span = span.clone();
            f(&mut span);
        });
    }

    fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let span = INVOCATION_SPAN.with(|span| span.clone());
        tokio::spawn(INVOCATION_SPAN.scope(span, future))
    }
}
