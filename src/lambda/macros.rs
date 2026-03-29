// make_lambda_runtime! macro — generates main(), tracer init, instrumented
// SDK clients, Tower layer setup, and Lambda runtime execution.

use opentelemetry_sdk::trace::SdkTracerProvider;

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
