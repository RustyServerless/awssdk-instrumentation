//! `tracing`-backend interceptor (`tracing-backend` feature).
//!
//! [`TracingInterceptor`] implements the AWS SDK `Intercept` trait by writing
//! span attributes into the active `tracing::Span`. The `tracing-opentelemetry`
//! bridge then forwards those attributes to the configured OTel exporter.
//!
//! This is the recommended backend for most workloads. It integrates naturally
//! with the `tracing` ecosystem and allows mixing AWS SDK spans with application
//! spans in the same trace.
//!
//! [`TracingInterceptor`] is re-exported as [`super::DefaultInterceptor`] when
//! `tracing-backend` is the active backend.

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

use tracing::Span;

use super::{
    DefaultExtractor,
    utils::{SpanPauser, StorableOption},
};

/// AWS SDK interceptor that writes OTel attributes into the active `tracing::Span`.
///
/// `TracingInterceptor` implements the AWS SDK [`Intercept`] trait and hooks
/// into the four SDK lifecycle phases (before serialization, after serialization,
/// before deserialization, after execution) to extract OTel semantic-convention
/// attributes from each SDK call.
///
/// Attributes are written to the `tracing::Span` that the AWS SDK creates for
/// the operation. The `tracing-opentelemetry` bridge then forwards those
/// attributes to the configured OTel exporter.
///
/// This is the recommended backend for most workloads. It integrates naturally
/// with the `tracing` ecosystem and allows mixing AWS SDK spans with application
/// spans in the same trace.
///
/// `TracingInterceptor` is re-exported as [`super::DefaultInterceptor`] when
/// `tracing-backend` is the active backend.
///
/// The `extractor` field is public so you can register custom hooks and
/// extractors before attaching the interceptor to a client config.
///
/// # Examples
///
/// ```no_run
/// # async fn example() {
/// use awssdk_instrumentation::{
///     interceptor::{
///         tracing::TracingInterceptor, ServiceFilter
///     },
///     span_write::SpanWrite,
/// };
///
/// let mut interceptor = TracingInterceptor::new();
///
/// // Log the table name for every DynamoDB GetItem call.
/// interceptor.extractor.register_input_hook(
///     ServiceFilter::Operation("DynamoDB", "GetItem"),
///     |_service, _operation, _input, span| {
///         span.set_attribute("app.table", "orders");
///     },
/// );
///
/// // Attach to an AWS SDK client config:
/// let dynamo_config = aws_sdk_dynamodb::config::Builder::from(&::aws_config::load_from_env().await)
///     .interceptor(interceptor)
///     .build();
/// # }
/// ```
///
/// [`Intercept`]: aws_smithy_runtime_api::client::interceptors::Intercept
#[derive(Debug)]
#[non_exhaustive]
pub struct TracingInterceptor {
    /// The attribute extractor used by this interceptor.
    ///
    /// Register custom hooks and extractors on this field before attaching the
    /// interceptor to an AWS SDK client config.
    pub extractor: DefaultExtractor<Span>,
}

impl Default for TracingInterceptor {
    /// Creates a `TracingInterceptor` with no custom hooks or extractors.
    fn default() -> Self {
        Self::new()
    }
}

impl TracingInterceptor {
    /// Creates a new `TracingInterceptor` with no custom hooks or extractors.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use awssdk_instrumentation::interceptor::tracing::TracingInterceptor;
    ///
    /// let interceptor = TracingInterceptor::new();
    /// ```
    pub fn new() -> Self {
        Self {
            extractor: DefaultExtractor::new(),
        }
    }
}

/// Implements the AWS SDK [`Intercept`] trait, hooking into the four SDK
/// lifecycle phases to extract OTel attributes from each SDK call.
///
/// [`Intercept`]: aws_smithy_runtime_api::client::interceptors::Intercept
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
            span.record("otel.kind", "client");
            self.extractor
                .read_before_execution(context, cfg, &mut span)?;

            cfg.interceptor_state().store_put(StorableOption::new(span));
            Ok(())
        } else {
            Err("No operation span found")?
        }
    }

    fn read_after_serialization(
        &self,
        context: &BeforeTransmitInterceptorContextRef<'_>,
        _runtime_components: &RuntimeComponents,
        cfg: &mut ConfigBag,
    ) -> Result<(), BoxError> {
        let mut so_span = std::mem::take(
            cfg.get_mut_from_interceptor_state::<StorableOption<Span>>()
                .ok_or("No StorableOption<Span> in the ConfigBag")?,
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
                .ok_or("No StorableOption<Span> in the ConfigBag")?,
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
                .ok_or("No StorableOption<Span> in the ConfigBag")?,
        );

        if let Some(span) = so_span.as_mut() {
            self.extractor.read_after_execution(context, cfg, span)?;
        }

        Ok(())
    }
}
