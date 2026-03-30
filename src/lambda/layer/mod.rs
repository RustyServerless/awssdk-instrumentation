//! Tower `Layer` and `Service` for per-invocation OTel spans in Lambda.
//!
//! The key types are:
//!
//! - [`TracingLayer`] — a Tower `Layer` that wraps the Lambda runtime
//!   service. Construct it with a flush callback and optionally configure the
//!   [`OTelFaasTrigger`] attribute.
//! - [`TracingService`] — the Tower `Service` produced by [`TracingLayer`]. You
//!   rarely need to name this type directly.
//! - [`FlushedFuture`] — the future returned by [`TracingService`]. It calls
//!   the flush callback in its `Drop` impl, ensuring the exporter is flushed
//!   even when the invocation future is cancelled.
//! - [`Instrumentor`] — the backend-specific trait that creates and manages
//!   the invocation span. [`TracingInstrumentor`] (tracing-backend) and
//!   [`OtelInstrumentor`] (otel-backend) are the two implementations.
//! - [`OTelFaasTrigger`] — enum for the `faas.trigger` OTel attribute
//!   (`Http`, `PubSub`, `Timer`, `Datasource`, `Other`).
//!
//! [`DefaultTracingLayer`] is a type alias for [`TracingLayer`] with the
//! default backend instrumentor, which is the most convenient way to construct
//! the layer.
//!
//! ## Per-invocation span attributes
//!
//! The layer automatically sets the following OTel attributes on each
//! invocation span:
//!
//! - `faas.trigger` — from [`OTelFaasTrigger`] (default: `Datasource`)
//! - `faas.invocation_id` — Lambda request ID
//! - `faas.coldstart` — `true` for the first invocation
//! - `cloud.account.id` — extracted from the invoked function ARN
//! - `cloud.resource_id` — the invoked function ARN
//!
//! When the `_X_AMZN_TRACE_ID` header is present, the X-Ray trace context is
//! propagated into the span.

mod utils;

#[cfg(feature = "tracing-backend")]
mod tracing;
#[cfg(feature = "tracing-backend")]
pub use tracing::TracingInstrumentor;

#[cfg(feature = "otel-backend")]
mod otel;
#[cfg(feature = "otel-backend")]
pub use otel::OtelInstrumentor;

pub use utils::OTelFaasTrigger;

use std::{marker::PhantomData, mem::ManuallyDrop, pin::Pin, task};
use tokio::task::JoinHandle;

use lambda_runtime::{
    LambdaInvocation, Service,
    tower::{BoxError, Layer},
};
use pin_project::{pin_project, pinned_drop};

use utils::XRayTraceHeader;

use crate::span_write::SpanWrite;

// Tower Layer for Lambda invocations — creates a span per invocation,
// extracts _X_AMZN_TRACE_ID, sets invocation attributes, flushes exporter.

/// Per-invocation context passed from the Tower layer to the backend [`Instrumentor`].
#[doc(hidden)]
#[derive(Debug)]
pub struct InvocationContext {
    xray_trace_header: Option<XRayTraceHeader>,
    function_arn: String,
    account_id: String,
    request_id: String,
    trigger: OTelFaasTrigger,
    is_coldstart: bool,
}

/// Backend-specific strategy for creating and managing per-invocation spans.
///
/// `Instrumentor` is implemented by [`TracingInstrumentor`] (for the
/// `tracing-backend` feature) and [`OtelInstrumentor`] (for the `otel-backend`
/// feature). You do not implement this trait yourself; use the
/// [`DefaultInstrumentor`] type alias to select the active backend
/// automatically.
///
/// The trait is used as a type parameter on [`TracingLayer`] and
/// [`TracingService`] to keep the span management logic separate from the Tower
/// middleware plumbing.
pub trait Instrumentor {
    /// The instrumented future type produced by [`instrument`].
    ///
    /// [`instrument`]: Instrumentor::instrument
    type IFut<F: Future>: InstrumentedFuture<Fut: Future<Output = F::Output>>;

    /// The span type used for the per-invocation span.
    type InvocationSpan: SpanWrite;

    /// Wraps `inner` in a backend-specific instrumented future that creates and
    /// manages the per-invocation span described by `context`.
    fn instrument<Fut: Future>(inner: Fut, context: InvocationContext) -> Self::IFut<Fut>;

    /// Calls `f` with a mutable reference to the current invocation span.
    ///
    /// This is used by the interceptor to write attributes onto the invocation
    /// span from within an async task that is a child of the invocation future.
    fn with_invocation_span(f: impl FnOnce(&mut Self::InvocationSpan));

    /// Spawns a future as a Tokio task, propagating the invocation span context.
    fn spawn<F>(future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static;
}

/// Marker trait for futures that carry an instrumentation span.
///
/// Implemented by the backend-specific future types returned by
/// [`Instrumentor::instrument`]. You do not implement this trait yourself.
pub trait InstrumentedFuture: Future {
    /// The concrete future type being instrumented.
    type Fut: Future;
}

/// The default [`Instrumentor`] for the active backend.
///
/// Resolves to [`TracingInstrumentor`] when `tracing-backend` is enabled, or
/// to [`OtelInstrumentor`] when only `otel-backend` is active. Use this alias
/// as the `I` type parameter of [`TracingLayer`] to avoid hard-coding a backend.
#[cfg(feature = "tracing-backend")]
pub type DefaultInstrumentor = TracingInstrumentor;

/// The default [`Instrumentor`] for the active backend.
///
/// Resolves to [`OtelInstrumentor`] when only `otel-backend` is active, or to
/// [`TracingInstrumentor`] when `tracing-backend` is enabled. Use this alias
/// as the `I` type parameter of [`TracingLayer`] to avoid hard-coding a backend.
#[cfg(all(feature = "otel-backend", not(feature = "tracing-backend")))]
pub type DefaultInstrumentor = OtelInstrumentor;

/// A [`TracingLayer`] pre-configured with the default backend instrumentor.
///
/// This is the most convenient way to construct the Tower layer for Lambda
/// invocation instrumentation. The `F` type parameter is the flush callback
/// type (typically inferred).
///
/// # Examples
///
/// ```no_run
/// use awssdk_instrumentation::lambda::layer::DefaultTracingLayer;
///
/// // Flush callback — called after each invocation future drops.
/// let layer = DefaultTracingLayer::new(|| {
///     // flush the tracer provider here
/// });
/// ```
pub type DefaultTracingLayer<F> = TracingLayer<F, DefaultInstrumentor>;

/// Tower [`Layer`] that wraps the Lambda runtime service with per-invocation OTel spans.
///
/// `TracingLayer` intercepts each [`LambdaInvocation`] and:
///
/// 1. Parses the `_X_AMZN_TRACE_ID` header and propagates the X-Ray trace
///    context into the new span.
/// 2. Creates a `SERVER`-kind span named after the Lambda function with the
///    `faas.trigger`, `faas.invocation_id`, `faas.coldstart`,
///    `cloud.account.id`, and `cloud.resource_id` attributes.
/// 3. Wraps the invocation future in a [`FlushedFuture`] that calls the flush
///    callback when the future drops, ensuring the exporter is flushed even
///    when the invocation is cancelled.
///
/// Use [`DefaultTracingLayer`] to avoid specifying the `I` type parameter
/// explicitly.
///
/// # Examples
///
/// ```no_run
/// use awssdk_instrumentation::lambda::layer::DefaultTracingLayer;
/// use awssdk_instrumentation::lambda::OTelFaasTrigger;
///
/// let layer = DefaultTracingLayer::new(|| { /* flush */ })
///     .with_trigger(OTelFaasTrigger::Datasource);
/// ```
///
/// [`Layer`]: lambda_runtime::tower::Layer
/// [`LambdaInvocation`]: lambda_runtime::LambdaInvocation
pub struct TracingLayer<F: Fn() + Clone, I: Instrumentor> {
    flush_fn: F,
    trigger: OTelFaasTrigger,
    _phantom: PhantomData<I>,
}

impl<F: Fn() + Clone, I: Instrumentor> TracingLayer<F, I> {
    /// Creates a new `TracingLayer` with the given flush callback.
    ///
    /// The `flush_fn` is called synchronously in the `Drop` impl of
    /// [`FlushedFuture`] after each invocation completes or is cancelled. Use
    /// it to call `tracer_provider.force_flush()`.
    ///
    /// The `faas.trigger` attribute defaults to [`OTelFaasTrigger::Datasource`].
    /// Call [`with_trigger`] to override it.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use awssdk_instrumentation::lambda::layer::DefaultTracingLayer;
    ///
    /// let layer = DefaultTracingLayer::new(|| { /* flush */ });
    /// ```
    ///
    /// [`with_trigger`]: TracingLayer::with_trigger
    pub fn new(flush_fn: F) -> Self {
        Self {
            flush_fn,
            trigger: OTelFaasTrigger::default(),
            _phantom: PhantomData,
        }
    }

    /// Sets the `faas.trigger` OTel attribute for every invocation span.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use awssdk_instrumentation::lambda::layer::DefaultTracingLayer;
    /// use awssdk_instrumentation::lambda::OTelFaasTrigger;
    ///
    /// let layer = DefaultTracingLayer::new(|| { /* flush */ })
    ///     .with_trigger(OTelFaasTrigger::Http);
    /// ```
    pub fn with_trigger(self, trigger: OTelFaasTrigger) -> Self {
        Self { trigger, ..self }
    }
}

/// Wraps the Lambda runtime service `S` with per-invocation OTel instrumentation.
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

/// Tower [`Service`] produced by [`TracingLayer`].
///
/// You rarely need to name this type directly. It is returned by
/// [`TracingLayer::layer`] and implements [`Service<LambdaInvocation>`].
///
/// Each call to [`Service::call`] creates a per-invocation OTel span, wraps
/// the inner service's future in a [`FlushedFuture`], and tracks whether the
/// invocation is a cold start.
///
/// [`Service`]: lambda_runtime::Service
/// [`Service::call`]: lambda_runtime::Service::call
pub struct TracingService<I: Instrumentor, S, F> {
    inner: S,
    flush_fn: F,
    coldstart: bool,
    trigger: OTelFaasTrigger,
    account_id: Option<String>,
    _phantom: PhantomData<I>,
}
/// Implements [`Service<LambdaInvocation>`] for [`TracingService`].
///
/// Each call parses the X-Ray trace header, builds an [`InvocationContext`],
/// instruments the inner service's future, and wraps it in a [`FlushedFuture`].
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

        // Next invocation won't be cold starts by definition
        self.coldstart = false;

        FlushedFuture {
            future: ManuallyDrop::new(I::instrument(self.inner.call(req), invocation_context)),
            flush_fn: self.flush_fn.clone(),
        }
    }
}

/// A future wrapper that calls a flush callback when it drops.
///
/// `FlushedFuture` is the future type returned by [`TracingService`]. It wraps
/// the backend-specific instrumented future and calls `flush_fn` in its `Drop`
/// impl, ensuring the OTel exporter is flushed after each Lambda invocation
/// even when the future is cancelled before it completes.
///
/// You do not construct `FlushedFuture` directly; it is produced by
/// [`TracingService::call`].
#[pin_project(PinnedDrop)]
pub struct FlushedFuture<Fut: InstrumentedFuture, F: Fn() + Clone> {
    #[pin]
    future: ManuallyDrop<Fut>,
    flush_fn: F,
}

/// Polls the inner instrumented future, propagating its output.
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
