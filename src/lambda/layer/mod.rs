mod utils;

#[cfg(feature = "tracing-backend")]
mod tracing;
#[cfg(feature = "tracing-backend")]
pub use tracing::TracingInstrumentor;

#[cfg(feature = "otel-backend")]
mod otel;
#[cfg(feature = "otel-backend")]
pub use otel::OtelInstrumentor;

use std::{marker::PhantomData, mem::ManuallyDrop, pin::Pin, task};

use lambda_runtime::{
    LambdaInvocation, Service,
    tower::{BoxError, Layer},
};
use pin_project::{pin_project, pinned_drop};
pub use utils::OTelFaasTrigger;
use utils::XRayTraceHeader;
// Tower Layer for Lambda invocations — creates a span per invocation,
// extracts _X_AMZN_TRACE_ID, sets invocation attributes, flushes exporter.

pub struct InvocationContext {
    xray_trace_header: Option<XRayTraceHeader>,
    function_arn: String,
    account_id: String,
    request_id: String,
    trigger: OTelFaasTrigger,
    is_coldstart: bool,
}

/// Trait for futures that are aware of instrumentation spans
pub trait Instrumentor {
    type IFut<F: Future>: InstrumentedFuture<Fut: Future<Output = F::Output>>;
    fn instrument<Fut: Future>(inner: Fut, context: InvocationContext) -> Self::IFut<Fut>;
}

pub trait InstrumentedFuture: Future {
    type Fut: Future;
}

#[cfg(feature = "tracing-backend")]
pub type DefaultInstrumentor = TracingInstrumentor;

#[cfg(all(feature = "otel-backend", not(feature = "tracing-backend")))]
pub type DefaultInstrumentor = OtelInstrumentor;

pub type DefaultTracingLayer<F> = TracingLayer<F, DefaultInstrumentor>;

/// Tower middleware to create a tracing span for invocations of the Lambda function.
pub struct TracingLayer<F: Fn() + Clone, I: Instrumentor> {
    flush_fn: F,
    trigger: OTelFaasTrigger,
    _phantom: PhantomData<I>,
}

impl<F: Fn() + Clone, I: Instrumentor> TracingLayer<F, I> {
    /// Create a new tracing layer.
    pub fn new(flush_fn: F) -> Self {
        Self {
            flush_fn,
            trigger: OTelFaasTrigger::default(),
            _phantom: PhantomData,
        }
    }
    /// Configure the `faas.trigger` attribute of the OpenTelemetry span.
    pub fn with_trigger(self, trigger: OTelFaasTrigger) -> Self {
        Self { trigger, ..self }
    }
}

impl<S, F: Fn() + Clone, I: Instrumentor> Layer<S> for TracingLayer<F, I> {
    type Service = TracingService<I, S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        TracingService {
            inner,
            flush_fn: self.flush_fn.clone(),
            coldstart: true,
            trigger: self.trigger,
            account_id: None,
            _phantom: PhantomData,
        }
    }
}

/// Tower service returned by [TracingLayer].
pub struct TracingService<I: Instrumentor, S, F> {
    inner: S,
    flush_fn: F,
    coldstart: bool,
    trigger: OTelFaasTrigger,
    account_id: Option<String>,
    _phantom: PhantomData<I>,
}
impl<I, S, F: Fn() + Clone> Service<LambdaInvocation> for TracingService<I, S, F>
where
    S: Service<LambdaInvocation, Response = (), Error = BoxError>,
    <I as Instrumentor>::IFut<<S as Service<LambdaInvocation>>::Future>:
        Future<Output = <<S as Service<LambdaInvocation>>::Future as Future>::Output>,
    I: Instrumentor,
{
    type Response = ();
    type Error = BoxError;
    type Future =
        FlushedFuture<<I as Instrumentor>::IFut<<S as Service<LambdaInvocation>>::Future>, F>;

    fn poll_ready(&mut self, cx: &mut task::Context<'_>) -> task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: LambdaInvocation) -> Self::Future {
        dbg!(&req.parts);
        dbg!(&req.body);
        dbg!(&req.context);

        let account_id = self
            .account_id
            .get_or_insert_with(|| {
                req.context
                    .invoked_function_arn
                    .split(':')
                    .nth(4)
                    .map(|v| v.to_owned())
                    .unwrap_or_default()
            })
            .to_owned();

        let xray_trace_header = req.context.xray_trace_id.as_ref().and_then(|trace_id| {
            trace_id
                .parse()
                .map_err(|e| log::warn!("Could not parse XRayTraceHeader: {e}"))
                .ok()
        });

        let invocation_context = InvocationContext {
            xray_trace_header,
            function_arn: req.context.invoked_function_arn.to_owned(),
            account_id,
            request_id: req.context.request_id.to_owned(),
            trigger: self.trigger,
            is_coldstart: self.coldstart,
        };

        // Will only be true the first time this is called
        self.coldstart = false;

        FlushedFuture {
            future: ManuallyDrop::new(I::instrument(self.inner.call(req), invocation_context)),
            flush_fn: self.flush_fn.clone(),
        }
    }
}

#[pin_project(PinnedDrop)]
pub struct FlushedFuture<Fut: InstrumentedFuture, F: Fn() + Clone> {
    #[pin]
    future: ManuallyDrop<Fut>,
    flush_fn: F,
}
impl<Fut: InstrumentedFuture, F: Fn() + Clone> Future for FlushedFuture<Fut, F> {
    type Output = Fut::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let this = self.project();
        // SAFETY: As long as `ManuallyDrop<T>` does not move, `T` won't move
        //         and `inner` is valid, because `ManuallyDrop::drop` is called
        //         only inside `Drop` of the `TracingProviderFlusher`.
        let future: Pin<&mut Fut> = unsafe { this.future.map_unchecked_mut(|fut| &mut **fut) };
        future.poll(cx)
    }
}

#[pinned_drop]
impl<Fut: InstrumentedFuture, F: Fn() + Clone> PinnedDrop for FlushedFuture<Fut, F> {
    fn drop(self: std::pin::Pin<&mut Self>) {
        let this = self.project();

        // SAFETY: 1. `Pin::get_unchecked_mut()` is safe, because this isn't
        //             different from wrapping `T` in `Option` and calling
        //             `Pin::set(&mut this.inner, None)`, except avoiding
        //             additional memory overhead.
        //         2. `ManuallyDrop::drop()` is safe, because
        //            `PinnedDrop::drop()` is guaranteed to be called only
        //            once.
        unsafe { ManuallyDrop::drop(this.future.get_unchecked_mut()) }
        (*this.flush_fn)();
    }
}
