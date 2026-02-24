// make_lambda_runtime! macro — generates main(), tracer init, instrumented
// SDK clients, Tower layer setup, and Lambda runtime execution.

pub use lambda_runtime;
pub use log;
pub use tokio;

#[macro_export]
macro_rules! make_lambda_runtime {
    (
        internal
        $handler:ident,
        tracer_provider = $tracer_provider:expr,
        trigger = $trigger:expr,
        runtime_layer = $runtime_layer:ty
        $(, $name:ident() -> $client:ty)+
    ) => {
        $crate::aws_sdk_config_provider!();
        $(
            $crate::aws_sdk_client_provider!($name() -> $client);
        )+
        make_lambda_runtime!(internal $handler, tracer_provider = $tracer_provider, trigger = $trigger, runtime_layer = $runtime_layer ; with_code sdk_config_init().await;);
    };
    (
        internal
        $handler:ident,
        tracer_provider = $tracer_provider:expr,
        trigger = $trigger:expr,
        runtime_layer = $runtime_layer:ty ;
        $(with_code $($code:tt)+)?
    ) => {
        #[$crate::lambda::macros::tokio::main]
        async fn main() -> Result<(), lambda_runtime::Error> {
            let tracer_provider = $tracer_provider;

            $($($code)+)?

            use $crate::lambda::macros::lambda_runtime;
            use $crate::lambda::macros::log;
            lambda_runtime::Runtime::new(lambda_runtime::service_fn($handler))
                .layer(
                    <$runtime_layer>::new(|| {
                        match tracer_provider.force_flush() {
                            Ok(_) => {
                                log::info!("TracingProviderFlusher: Flushed tracing provider");
                            }
                            Err(e) => {
                                log::warn!("Could not flush tracing provider: {e}");
                            }
                        }
                    })
                    .with_trigger($trigger),
                )
                .run()
                .await
        }
    };
    // All 3 optional args present, 6 combinations
    (
        $handler:ident,
        tracer_provider = $tracer_provider:expr,
        trigger = $trigger:expr,
        runtime_layer = $runtime_layer:ty
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $tracer_provider, trigger = $trigger, runtime_layer = $runtime_layer $(,$name() -> $client)*);
    };
    (
        $handler:ident,
        tracer_provider = $tracer_provider:expr,
        runtime_layer = $runtime_layer:ty,
        trigger = $trigger:expr
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $tracer_provider, trigger = $trigger, runtime_layer = $runtime_layer $(,$name() -> $client)*);
    };
    (
        $handler:ident,
        runtime_layer = $runtime_layer:ty,
        tracer_provider = $tracer_provider:expr,
        trigger = $trigger:expr
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $tracer_provider, trigger = $trigger, runtime_layer = $runtime_layer $(,$name() -> $client)*);
    };
    (
        $handler:ident,
        runtime_layer = $runtime_layer:ty,
        trigger = $trigger:expr,
        tracer_provider = $tracer_provider:expr
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $tracer_provider, trigger = $trigger, runtime_layer = $runtime_layer $(,$name() -> $client)*);
    };
    (
        $handler:ident,
        trigger = $trigger:expr,
        tracer_provider = $tracer_provider:expr,
        runtime_layer = $runtime_layer:ty
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $tracer_provider, trigger = $trigger, runtime_layer = $runtime_layer $(,$name() -> $client)*);
    };
    (
        $handler:ident,
        trigger = $trigger:expr,
        runtime_layer = $runtime_layer:ty,
        tracer_provider = $tracer_provider:expr
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $tracer_provider, trigger = $trigger, runtime_layer = $runtime_layer $(,$name() -> $client)*);
    };
    // Only runtime_layer and trigger, 2 combinations
    (
        $handler:ident,
        trigger = $trigger:expr,
        runtime_layer = $runtime_layer:ty
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $crate::init::default_telemetry_init(), trigger = $trigger, runtime_layer = $runtime_layer $(,$name() -> $client)*);
    };
    (
        $handler:ident,
        runtime_layer = $runtime_layer:ty,
        trigger = $trigger:expr
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $crate::init::default_telemetry_init(), trigger = $trigger, runtime_layer = $runtime_layer $(,$name() -> $client)*);
    };
    // Only runtime_layer and tracer_provider, 2 combinations
    (
        $handler:ident,
        runtime_layer = $runtime_layer:ty,
        tracer_provider = $tracer_provider:expr
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $tracer_provider, trigger = $crate::lambda::layer::OTelFaasTrigger::Http, runtime_layer = $runtime_layer $(,$name() -> $client)*);
    };
    (
        $handler:ident,
        tracer_provider = $tracer_provider:expr,
        runtime_layer = $runtime_layer:ty
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $tracer_provider, trigger = $crate::lambda::layer::OTelFaasTrigger::Http, runtime_layer = $runtime_layer $(,$name() -> $client)*);
    };
    // Only tracer_provider and trigger, 2 combinations
    (
        $handler:ident,
        trigger = $trigger:expr,
        tracer_provider = $tracer_provider:expr
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $tracer_provider, trigger = $trigger, runtime_layer = $crate::lambda::layer::DefaultTracingLayer<_> $(,$name() -> $client)*);
    };
    (
        $handler:ident,
        tracer_provider = $tracer_provider:expr,
        trigger = $trigger:expr
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $tracer_provider, trigger = $trigger, runtime_layer = $crate::lambda::layer::DefaultTracingLayer<_> $(,$name() -> $client)*);
    };
    // Only one optional parameter, 3 combinations
    (
        $handler:ident,
        trigger = $trigger:expr
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $crate::init::default_telemetry_init(), trigger = $trigger, runtime_layer = $crate::lambda::layer::DefaultTracingLayer<_> $(,$name() -> $client)*);
    };
    (
        $handler:ident,
        tracer_provider = $tracer_provider:expr
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $tracer_provider, trigger = $crate::lambda::layer::OTelFaasTrigger::Http, runtime_layer = $crate::lambda::layer::DefaultTracingLayer<_> $(,$name() -> $client)*);
    };
    (
        $handler:ident,
        runtime_layer = $runtime_layer:ty
        $(, $name:ident() -> $client:ty)*
    ) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $crate::init::default_telemetry_init(), trigger = $crate::lambda::layer::OTelFaasTrigger::Http, runtime_layer = $runtime_layer $(,$name() -> $client)*);
    };
    // No optional parameter
    ($handler:ident $(, $name:ident() -> $client:ty)*) => {
        make_lambda_runtime!(internal $handler, tracer_provider = $crate::init::default_telemetry_init(), trigger = $crate::lambda::layer::OTelFaasTrigger::Http, runtime_layer = $crate::lambda::layer::DefaultTracingLayer<_> $(,$name() -> $client)*);
    };
}
