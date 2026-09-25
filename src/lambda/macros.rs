//! The [`make_lambda_runtime!`] macro and its `default_flush_tracer` helper.
//!
//! [`make_lambda_runtime!`] generates a complete `#[tokio::main] async fn main()`
//! that wires together telemetry initialisation, optional SDK client singletons,
//! and the Lambda runtime.
//!
//! [`make_lambda_runtime!`]: crate::make_lambda_runtime

use opentelemetry_sdk::trace::SdkTracerProvider;

/// Flushes the given [`SdkTracerProvider`], logging the outcome.
///
/// Called by the [`make_lambda_runtime!`]-generated flush closure after each invocation.
#[doc(hidden)]
pub fn default_flush_tracer(tracer_provider: &SdkTracerProvider) {
    match tracer_provider.force_flush() {
        Ok(_) => {
            log::info!("TracingProviderFlusher: Flushed tracing provider");
        }
        Err(e) => {
            log::warn!("Could not flush tracing provider: {e}");
        }
    }
}

/// Generates a complete `#[tokio::main] async fn main()` for a Lambda function.
///
/// This macro wires together:
/// - telemetry initialisation,
/// - optional AWS SDK client singletons with automatic operation-span generation,
/// - Lambda runtime with a [`TracingLayer`] applied.
///
/// It is the recommended entry point for Lambda functions.
///
/// # Syntax
///
/// ```text
/// make_lambda_runtime!(
///     handler
///     [, extra_init_code = { ... }]
///     [, trigger = OTelFaasTrigger::Variant]
///     [, telemetry_init = my_telemetry_init_fn]
///     [, sdk_client_interceptor = CustomInterceptor::new()]
///     [, lambda_layer_instrumentor = CustomInstrumentor]
///     [, client_fn() -> SdkClientType]*
/// );
/// ```
///
/// All parameters after `handler` are optional and can appear in any order:
///
/// - **`handler`** *(required)* — path to the async handler function.
/// - **`extra_init_code`** — arbitrary code delimited in {} that will be executed
///   in `main()` just before the Lambda runtime and after every other generated
///   initialization steps.
/// - **`trigger`** — the [`OTelFaasTrigger`] variant for the `faas.trigger`
///   attribute. Defaults to [`OTelFaasTrigger::default`].
/// - **`telemetry_init`** — a custom telemetry init function with signature
///   `fn() -> SdkTracerProvider`. Defaults to [`default_telemetry_init`].
/// - **`sdk_client_interceptor`** — the [`Intercept`] type to use instead of [`DefaultInterceptor`] for AWS SDK clients,
///   typically an explicit [`TracingInterceptor`] or [`OtelInterceptor`].
/// - **`lambda_layer_instrumentor`** — the [`Instrumentor`] of the [`TracingLayer`]
///   to use instead of [`DefaultInstrumentor`], typically [`TracingInstrumentor`] or [`OtelInstrumentor`].
/// - **`client_fn() -> SdkClientType`** — zero or more SDK client declarations.
///   Each generates a `OnceLock`-backed accessor that returns the AWS SDK client
///   with an interceptor type implementing [`Intercept`] pre-attached that will generate
///   appropriate spans.
///
/// # Prerequisites
///
/// `tokio` must be a direct dependency of your crate. Lambda functions in Rust
/// need `tokio` anyway.
///
/// # Examples
///
/// Minimal usage — just the handler:
///
/// ```no_run
/// use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent};
/// use serde_json::Value;
///
/// async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
///     Ok(event.payload)
/// }
///
/// awssdk_instrumentation::make_lambda_runtime!(handler);
/// ```
///
/// With a DynamoDB client and a datasource trigger:
///
/// ```no_run
/// # mod private {
/// use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent, OTelFaasTrigger};
/// use serde_json::Value;
///
/// async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
///     let _client = dynamodb_client();
///     Ok(event.payload)
/// }
///
/// awssdk_instrumentation::make_lambda_runtime!(
///     handler,
///     trigger = OTelFaasTrigger::Datasource,
///     dynamodb_client() -> aws_sdk_dynamodb::Client
/// );
/// # }
/// # fn dynamodb_client() {}
/// # fn aws_sdk_config() {}
/// # fn main() {}
/// ```
///
/// [`DefaultTracingLayer`]: crate::lambda::layer::DefaultTracingLayer
/// [`OTelFaasTrigger`]: crate::lambda::OTelFaasTrigger
/// [`OTelFaasTrigger::default`]: crate::lambda::OTelFaasTrigger::default
/// [`default_telemetry_init`]: crate::init::default_telemetry_init
/// [`Intercept`]: aws_smithy_runtime_api::client::interceptors::Intercept
/// [`DefaultInterceptor`]: crate::interceptor::DefaultInterceptor
/// [`TracingInterceptor`]: crate::interceptor::tracing::TracingInterceptor
/// [`OtelInterceptor`]: crate::interceptor::otel::OtelInterceptor
/// [`TracingLayer`]: crate::lambda::layer::TracingLayer
/// [`Instrumentor`]: crate::lambda::layer::Instrumentor
/// [`DefaultInstrumentor`]: crate::lambda::layer::DefaultInstrumentor
/// [`TracingInstrumentor`]: crate::lambda::layer::TracingInstrumentor
/// [`OtelInstrumentor`]: crate::lambda::layer::OtelInstrumentor
/// [`aws_sdk_config_provider!`]: crate::aws_sdk_config_provider
#[macro_export]
macro_rules! make_lambda_runtime {
    // extra_init_code
    (
        [$($found:tt)+]
        @find_extra_init_code
        $(@$other_ats:ident)*
        [
            extra_init_code = {$($extra_init_code:tt)*},
            $($candidates:tt)*
        ]
        $($after:tt)*
    ) => {
        $crate::make_lambda_runtime!(
            [$($found)+ extra_init_code = {$($extra_init_code)*},]
            $(@$other_ats)*
            [$($after)* $($candidates)*]
        );
    };
    (
        [$($found:tt)+]
        @find_extra_init_code
        $(@$other_ats:ident)*
        []
        $($after:tt)*
    ) => {
        $crate::make_lambda_runtime!(
            [$($found)+ extra_init_code = {},]
            $(@$other_ats)*
            [$($after)*]
        );
    };

    // telemetry_init
    (
        [$($found:tt)+]
        @find_telemetry_init
        $(@$other_ats:ident)*
        [
            telemetry_init = $telemetry_init:path,
            $($candidates:tt)*
        ]
        $($after:tt)*
    ) => {
        $crate::make_lambda_runtime!(
            [$($found)+ telemetry_init = $telemetry_init,]
            $(@$other_ats)*
            [$($after)* $($candidates)*]
        );
    };
    (
        [$($found:tt)+]
        @find_telemetry_init
        $(@$other_ats:ident)*
        []
        $($after:tt)*
    ) => {
        $crate::make_lambda_runtime!(
            [$($found)+ telemetry_init = $crate::init::default_telemetry_init,]
            $(@$other_ats)*
            [$($after)*]
        );
    };

    // trigger
    (
        [$($found:tt)+]
        @find_trigger
        $(@$other_ats:ident)*
        [
            trigger = $trigger:expr,
            $($candidates:tt)*
        ]
        $($after:tt)*
    ) => {
        $crate::make_lambda_runtime!(
            [$($found)+ trigger = $trigger,]
            $(@$other_ats)*
            [$($after)* $($candidates)*]
        );
    };
    (
        [$($found:tt)+]
        @find_trigger
        $(@$other_ats:ident)*
        []
        $($after:tt)*
    ) => {
        $crate::make_lambda_runtime!(
            [$($found)+ trigger = $crate::lambda::layer::OTelFaasTrigger::default(),]
            $(@$other_ats)*
            [$($after)*]
        );
    };

    // sdk_client_interceptor
    (
        [$($found:tt)+]
        @find_sdk_client_interceptor
        $(@$other_ats:ident)*
        [
            sdk_client_interceptor = $sdk_client_interceptor:expr,
            $($candidates:tt)*
        ]
        $($after:tt)*
    ) => {
        $crate::make_lambda_runtime!(
            [$($found)+ sdk_client_interceptor = $sdk_client_interceptor,]
            $(@$other_ats)*
            [$($after)* $($candidates)*]
        );
    };
    (
        [$($found:tt)+]
        @find_sdk_client_interceptor
        $(@$other_ats:ident)*
        []
        $($after:tt)*
    ) => {
        $crate::make_lambda_runtime!(
            [$($found)+ sdk_client_interceptor = $crate::interceptor::DefaultInterceptor::new(),]
            $(@$other_ats)*
            [$($after)*]
        );
    };


    // lambda_layer_instrumentor
    (
        [$($found:tt)+]
        @find_lambda_layer_instrumentor
        $(@$other_ats:ident)*
        [
            lambda_layer_instrumentor = $lambda_layer_instrumentor:ty,
            $($candidates:tt)*
        ]
        $($after:tt)*
    ) => {
        $crate::make_lambda_runtime!(
            [$($found)+ lambda_layer_instrumentor = $lambda_layer_instrumentor,]
            $(@$other_ats)*
            [$($after)* $($candidates)*]
        );
    };
    (
        [$($found:tt)+]
        @find_lambda_layer_instrumentor
        $(@$other_ats:ident)*
        []
        $($after:tt)*
    ) => {
        $crate::make_lambda_runtime!(
            [$($found)+ lambda_layer_instrumentor = $crate::lambda::layer::DefaultInstrumentor,]
            $(@$other_ats)*
            [$($after)*]
        );
    };

    // Cursor avancement
    (
        [$($found:tt)+]
        $(@$ats:ident)+
        [$t:tt $($candidates:tt)*]
        $($after:tt)*
    ) => {
        $crate::make_lambda_runtime!(
            [$($found)+]
            $(@$ats)+
            [$($candidates)*]
            $($after)*
            $t
        );
    };

    // Exit finding phase
    (
        [$($found:tt)+]
        [$($rest:tt)*]
    ) => {
        $crate::make_lambda_runtime!(
            internal
            $($found)+
            $($rest)*
        );
    };

    // Final internal processing
    (
        internal
        $handler:path,
        extra_init_code = {$($extra_init_code:tt)*},
        telemetry_init = $telemetry_init:path,
        trigger = $trigger:expr,
        sdk_client_interceptor = $sdk_client_interceptor:expr,
        lambda_layer_instrumentor = $lambda_layer_instrumentor:ty,
    ) => {
        $crate::make_lambda_runtime!(
            internal
            $handler,
            telemetry_init = $telemetry_init,
            trigger = $trigger,
            lambda_layer_instrumentor = $lambda_layer_instrumentor ;
            $($extra_init_code)*
        );
    };
    (
        internal
        $handler:path,
        extra_init_code = {$($extra_init_code:tt)*},
        telemetry_init = $telemetry_init:path,
        trigger = $trigger:expr,
        sdk_client_interceptor = $sdk_client_interceptor:expr,
        lambda_layer_instrumentor = $lambda_layer_instrumentor:ty,
        $($name:ident() -> $client:ty,)+
    ) => {
        $crate::aws_sdk_config_provider!();
        $(
            $crate::aws_sdk_client_provider!($name() -> $client, interceptor = $sdk_client_interceptor);
        )+
        $crate::make_lambda_runtime!(
            internal
            $handler,
            telemetry_init = $telemetry_init,
            trigger = $trigger,
            lambda_layer_instrumentor = $lambda_layer_instrumentor ;
            sdk_config_init().await; $($extra_init_code)*
        );
    };
    (
        internal
        $handler:path,
        telemetry_init = $telemetry_init:path,
        trigger = $trigger:expr,
        lambda_layer_instrumentor = $lambda_layer_instrumentor:ty ;
        $($code:tt)*
    ) => {
        #[tokio::main]
        async fn main() -> Result<(), $crate::lambda::lambda_runtime::Error> {

            const _: fn() = || {
                fn _test_telemetry_init(_f: fn() -> $crate::opentelemetry_sdk::trace::SdkTracerProvider) {}
                _test_telemetry_init($telemetry_init)
            };
            let tracer_provider = $telemetry_init();

            $($code)*

            $crate::lambda::lambda_runtime::Runtime::new($crate::lambda::lambda_runtime::service_fn($handler))
                .layer(
                    <$crate::lambda::layer::TracingLayer<_, $lambda_layer_instrumentor>>::new(move || {$crate::lambda::macros::default_flush_tracer(&tracer_provider);})
                    .with_trigger($trigger)
                )
                .run()
                .await
        }
    };
    // Entry point
    ($handler:path $(,$($rest:tt)+)?) => {
        $crate::make_lambda_runtime!(
            [$handler, ]
            @find_extra_init_code
            @find_telemetry_init
            @find_trigger
            @find_sdk_client_interceptor
            @find_lambda_layer_instrumentor
            [$($($rest)+,)?]
        );
    };
}
