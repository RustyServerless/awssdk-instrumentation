//! The [`make_lambda_runtime!`] macro and its `default_flush_tracer` helper.
//!
//! [`make_lambda_runtime!`] generates a complete `#[tokio::main] async fn main()`
//! that wires together telemetry initialisation, optional SDK client singletons,
//! and the Lambda runtime with the [`super::layer::DefaultTracingLayer`] applied.
//!
//! ## Macro syntax
//!
//! ```text
//! make_lambda_runtime!(
//!     handler_fn
//!     [, trigger = OTelFaasTrigger::Http]
//!     [, telemetry_init = my_telemetry_init]
//!     [, client_fn() -> SdkClientType]*
//! );
//! ```
//!
//! All parameters after `handler_fn` are optional:
//!
//! - `trigger` — sets the `faas.trigger` attribute (default: `Http`)
//! - `telemetry_init` — custom telemetry init function with signature
//!   `fn() -> SdkTracerProvider` (default: [`crate::init::default_telemetry_init`])
//! - `client_fn() -> SdkClientType` — zero or more SDK client declarations;
//!   each generates a `OnceLock`-backed accessor with
//!   [`crate::interceptor::DefaultInterceptor`] pre-attached
//!
//! ## Example
//!
//! ```no_run
//! use lambda_runtime::{Error, LambdaEvent};
//! use serde_json::Value;
//!
//! async fn handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
//!     Ok(event.payload)
//! }
//!
//! // Minimal: just the handler, defaults to Http trigger.
//! awssdk_instrumentation::make_lambda_runtime!(handler);
//! ```
//!
//! [`make_lambda_runtime!`]: crate::make_lambda_runtime

// make_lambda_runtime! macro — generates main(), tracer init, instrumented
// SDK clients, Tower layer setup, and Lambda runtime execution.

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
/// This macro wires together telemetry initialisation, optional AWS SDK client
/// singletons, and the Lambda runtime with the [`DefaultTracingLayer`] applied.
/// It is the recommended entry point for Lambda functions using this crate.
///
/// # Syntax
///
/// ```text
/// make_lambda_runtime!(
///     handler_fn
///     [, trigger = OTelFaasTrigger::Variant]
///     [, telemetry_init = my_telemetry_init_fn]
///     [, client_fn() -> SdkClientType]*
/// );
/// ```
///
/// All parameters after `handler_fn` are optional and can appear in any order:
///
/// - **`handler_fn`** *(required)* — path to the async handler function.
/// - **`trigger`** — the [`OTelFaasTrigger`] variant for the `faas.trigger`
///   attribute. Defaults to [`OTelFaasTrigger::Http`].
/// - **`telemetry_init`** — a custom telemetry init function with signature
///   `fn() -> SdkTracerProvider`. Defaults to [`default_telemetry_init`].
/// - **`client_fn() -> SdkClientType`** — zero or more SDK client declarations.
///   Each generates a `OnceLock`-backed accessor with [`DefaultInterceptor`]
///   pre-attached.
///
/// # Examples
///
/// Minimal usage — just the handler:
///
/// ```no_run
/// use lambda_runtime::{Error, LambdaEvent};
/// use serde_json::Value;
///
/// async fn handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
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
/// use lambda_runtime::{Error, LambdaEvent};
/// use serde_json::Value;
/// use awssdk_instrumentation::lambda::OTelFaasTrigger;
///
/// async fn handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
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
/// [`OTelFaasTrigger::Http`]: crate::lambda::OTelFaasTrigger::Http
/// [`default_telemetry_init`]: crate::init::default_telemetry_init
/// [`DefaultInterceptor`]: crate::interceptor::DefaultInterceptor
/// [`aws_sdk_config_provider!`]: crate::aws_sdk_config_provider
#[macro_export]
macro_rules! make_lambda_runtime {
    (
        internal
        $handler:path,
        telemetry_init = $telemetry_init:path,
        trigger = $trigger:expr
    ) => {
        $crate::make_lambda_runtime!(internal $handler, telemetry_init = $telemetry_init, trigger = $trigger ;);
    };
    (
        internal
        $handler:path,
        telemetry_init = $telemetry_init:path,
        trigger = $trigger:expr
        $(, $name:ident() -> $client:ty)+
    ) => {
        $crate::aws_sdk_config_provider!();
        $(
            $crate::aws_sdk_client_provider!($name() -> $client);
        )+
        $crate::make_lambda_runtime!(internal $handler, telemetry_init = $telemetry_init, trigger = $trigger ; with_code sdk_config_init().await;);
    };
    (
        internal
        $handler:path,
        telemetry_init = $telemetry_init:path,
        trigger = $trigger:expr ;
        $(with_code $($code:tt)+)?
    ) => {

        #[tokio::main]
        async fn main() -> Result<(), $crate::lambda::lambda_runtime::Error> {

            const _: fn() = || {
                fn _test_telemetry_init(_f: fn() -> $crate::lambda::opentelemetry_sdk::trace::SdkTracerProvider) {}
                _test_telemetry_init($telemetry_init)
            };
            let tracer_provider = $telemetry_init();

            $($($code)+)?


            $crate::lambda::lambda_runtime::Runtime::new($crate::lambda::lambda_runtime::service_fn($handler))
                .layer(
                    <$crate::lambda::layer::DefaultTracingLayer<_>>::new(move || {$crate::lambda::macros::default_flush_tracer(&tracer_provider);})
                    .with_trigger($trigger)
                )
                .run()
                .await
        }
    };
    // tracer_provider and trigger, 2 combinations
    (
        $handler:path,
        trigger = $trigger:expr,
        telemetry_init = $telemetry_init:path
        $(, $name:ident() -> $client:ty)*
    ) => {
        $crate::make_lambda_runtime!(internal $handler, telemetry_init = $telemetry_init, trigger = $trigger $(,$name() -> $client)*);
    };
    (
        $handler:path,
        telemetry_init = $telemetry_init:path,
        trigger = $trigger:expr
        $(, $name:ident() -> $client:ty)*
    ) => {
        $crate::make_lambda_runtime!(internal $handler, telemetry_init = $telemetry_init, trigger = $trigger $(,$name() -> $client)*);
    };
    // Only one optional parameter, 2 possibilities
    (
        $handler:path,
        trigger = $trigger:expr
        $(, $name:ident() -> $client:ty)*
    ) => {
        $crate::make_lambda_runtime!(internal $handler, telemetry_init = $crate::init::default_telemetry_init, trigger = $trigger $(,$name() -> $client)*);
    };
    (
        $handler:path,
        telemetry_init = $telemetry_init:path
        $(, $name:ident() -> $client:ty)*
    ) => {
        $crate::make_lambda_runtime!(internal $handler, telemetry_init = $telemetry_init, trigger = $crate::lambda::layer::OTelFaasTrigger::Http $(,$name() -> $client)*);
    };
    // No optional parameter
    ($handler:path $(, $name:ident() -> $client:ty)*) => {
        $crate::make_lambda_runtime!(internal $handler, telemetry_init = $crate::init::default_telemetry_init, trigger = $crate::lambda::layer::OTelFaasTrigger::Http $(,$name() -> $client)*);
    };
}
