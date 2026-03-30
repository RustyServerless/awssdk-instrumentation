//! OTel-native backend interceptor (`otel-backend` feature).
//!
//! [`OtelInterceptor`] implements the AWS SDK `Intercept` trait by creating and
//! managing OTel spans directly via the `opentelemetry` API, without going
//! through `tracing`. Each SDK call gets its own `CLIENT`-kind span named
//! `Service.Operation` (e.g. `DynamoDB.GetItem`).
//!
//! Use this backend when you want to avoid a `tracing` dependency or when you
//! need direct control over the OTel span lifecycle.
//!
//! [`OtelInterceptor`] is re-exported as [`super::DefaultInterceptor`] when
//! `otel-backend` is the only active backend.

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
    KeyValue,
    global::BoxedSpan,
    trace::{Span as SpanTrait, SpanBuilder, SpanKind, Tracer},
};
use opentelemetry_semantic_conventions::attribute as semco;

use super::{
    DefaultExtractor,
    utils::{StorableOption, extract_service_operation},
};

/// AWS SDK interceptor that creates and manages OTel spans directly via the
/// `opentelemetry` API.
///
/// `OtelInterceptor` implements the AWS SDK [`Intercept`] trait and hooks into
/// the four SDK lifecycle phases (before serialization, after serialization,
/// before deserialization, after execution) to extract OTel semantic-convention
/// attributes from each SDK call.
///
/// Unlike [`super::tracing::TracingInterceptor`], this backend does not go
/// through `tracing`. Each SDK call gets its own `CLIENT`-kind OTel span named
/// `Service.Operation` (e.g. `DynamoDB.GetItem`), created and ended directly
/// via the global OTel tracer.
///
/// Use this backend when you want to avoid a `tracing` dependency or when you
/// need direct control over the OTel span lifecycle.
///
/// `OtelInterceptor` is re-exported as [`super::DefaultInterceptor`] when
/// `otel-backend` is the only active backend.
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
///         otel::OtelInterceptor, ServiceFilter
///     },
///     span_write::SpanWrite,
/// };
///
/// let mut interceptor = OtelInterceptor::new();
///
/// // Tag every DynamoDB GetItem span with the target table.
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
pub struct OtelInterceptor {
    /// The attribute extractor used by this interceptor.
    ///
    /// Register custom hooks and extractors on this field before attaching the
    /// interceptor to an AWS SDK client config.
    pub extractor: DefaultExtractor<BoxedSpan>,
}

impl Default for OtelInterceptor {
    /// Creates an `OtelInterceptor` with no custom hooks or extractors.
    fn default() -> Self {
        Self::new()
    }
}

impl OtelInterceptor {
    /// Creates a new `OtelInterceptor` with no custom hooks or extractors.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use awssdk_instrumentation::interceptor::otel::OtelInterceptor;
    ///
    /// let interceptor = OtelInterceptor::new();
    /// ```
    pub fn new() -> Self {
        Self {
            extractor: DefaultExtractor::new(),
        }
    }
}

/// Implements the AWS SDK [`Intercept`] trait, hooking into the four SDK
/// lifecycle phases to create and populate an OTel span for each SDK call.
///
/// [`Intercept`]: aws_smithy_runtime_api::client::interceptors::Intercept
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
        let mut span = opentelemetry::global::tracer("").build(
            SpanBuilder::from_name("Service.Operation place holder")
                .with_start_time(start_time)
                .with_kind(SpanKind::Client)
                .with_attributes(vec![
                    KeyValue::new(semco::RPC_SYSTEM, "aws-api"),
                    KeyValue::new(super::RPC_SYSTEM_NAME, "aws-api"),
                ]),
        );

        self.extractor
            .read_before_execution(context, cfg, &mut span)?;

        // That's only available *AFTER* extractor.read_before_execution
        let (service, operation) = extract_service_operation(cfg);

        span.update_name(format!("{service}.{operation}"));
        span.set_attributes([
            KeyValue::new(semco::RPC_SERVICE, service.to_owned()),
            KeyValue::new(semco::RPC_METHOD, operation.to_owned()),
        ]);

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
                .ok_or("No StorableOption<BoxedSpan> in the ConfigBag")?,
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
                .ok_or("No StorableOption<BoxedSpan> in the ConfigBag")?,
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
                .ok_or("No StorableOption<BoxedSpan> in the ConfigBag")?,
        );

        if let Some(span) = so_span.as_mut() {
            self.extractor.read_after_execution(context, cfg, span)?;
        }

        Ok(())
    }
}
