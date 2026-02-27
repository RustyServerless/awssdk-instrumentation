// TracerProvider builder — sensible-default initialization with support for
// user overrides (span processor, exporter, resource, propagator).

use opentelemetry::{global, trace::Tracer as OtelTracer};
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
#[cfg(feature = "tracing-backend")]
use tracing::Subscriber;
#[cfg(feature = "tracing-backend")]
use tracing_subscriber::{Layer, registry::LookupSpan};

use crate::env::default_resource;

#[inline(always)]
pub fn default_telemetry_init() -> SdkTracerProvider {
    let tracer_provider = default_tracer_provider();
    global::set_tracer_provider(tracer_provider.clone());

    #[cfg(feature = "tracing-backend")]
    {
        use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

        let tracer = global::tracer("");
        let otel_layer = default_tracing_otel_layer(tracer);

        // Initialize tracing
        let console_layer = default_tracing_console_layer();

        // Use the tracing subscriber `Registry`, or any other subscriber
        // that impls `LookupSpan`
        tracing_subscriber::registry()
            .with(otel_layer)
            .with(console_layer)
            .init();
    }

    tracer_provider
}

#[inline(always)]
pub fn default_tracer_provider() -> SdkTracerProvider {
    let builder = SdkTracerProvider::builder()
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::AlwaysOn)))
        .with_resource(default_resource());

    #[cfg(feature = "export-xray-daemon")]
    let builder = {
        use opentelemetry_aws::{
            trace::XrayIdGenerator,
            xray_exporter::{XrayExporter, daemon_client::XrayDaemonClient},
        };
        builder
            .with_id_generator(XrayIdGenerator::default())
            .with_batch_exporter(XrayExporter::new(XrayDaemonClient::default()))
    };

    builder.build()
}

#[cfg(feature = "tracing-backend")]
#[inline(always)]
pub fn default_tracing_otel_layer<S, Tracer>(tracer: Tracer) -> impl Layer<S>
where
    Tracer: OtelTracer + 'static,
    Tracer::Span: Send + Sync,
    S: Subscriber + for<'any> LookupSpan<'any>,
{
    use tracing::Level;
    use tracing_subscriber::filter::filter_fn;

    tracing_opentelemetry::layer()
        .with_tracer(tracer)
        .with_threads(false)
        .with_filter(filter_fn(|metadata| {
            *metadata.level() <= Level::INFO
                || metadata.is_span()
                    && (metadata.target().contains("::operation::")
                        || metadata.target().ends_with("::tracing_runtime_layer"))
        }))
}

#[cfg(feature = "tracing-backend")]
#[inline(always)]
pub fn default_tracing_console_layer<S>() -> impl Layer<S>
where
    S: Subscriber + for<'any> LookupSpan<'any>,
{
    use tracing_subscriber::{filter::EnvFilter, fmt};
    fmt::layer()
        .json()
        .with_target(false)
        .with_ansi(false)
        .with_filter(EnvFilter::from_default_env())
}

#[macro_export]
#[allow(clippy::crate_in_macro_def)]
macro_rules! aws_sdk_config_provider {
    () => {
        static AWS_SDK_CONFIG: ::std::sync::OnceLock<::aws_config::SdkConfig> =
            ::std::sync::OnceLock::new();
        fn aws_sdk_config() -> &'static ::aws_config::SdkConfig {
            AWS_SDK_CONFIG.get().unwrap()
        }

        const _: fn() = || {
            // Compile-time assertion that `aws_sdk_config` is declared at the root
            crate::aws_sdk_config();
        };

        #[deny(dead_code)]
        async fn sdk_config_init() {
            AWS_SDK_CONFIG
                .set(::aws_config::load_from_env().await)
                .unwrap();
        }
    };
}

#[macro_export]
#[allow(clippy::crate_in_macro_def)]
macro_rules! aws_sdk_client_provider {
    ($name:ident() -> $client:ty, interceptor = $interceptor:expr) => {
        fn $name() -> $client {
            static CLIENT: ::std::sync::OnceLock<$client> = ::std::sync::OnceLock::new();
            CLIENT
                .get_or_init(|| {
                    let config = aws_sdk_config().into();
                    // The false branch is to force type inference of `config`
                    // to the correct one for the given Client type
                    if false {
                        <$client>::from_conf(config)
                    } else {
                        <$client>::from_conf(config.to_builder().interceptor($interceptor).build())
                    }
                })
                .clone()
        }
        const _: fn() = || {
            // Compile-time assertion that function is declared at the root
            crate::$name();
        };
    };
    ($name:ident() -> $client:ty, no_interceptor) => {
        fn $name() -> $client {
            static CLIENT: ::std::sync::OnceLock<$client> = ::std::sync::OnceLock::new();
            CLIENT
                .get_or_init(|| <$client>::new($crate::aws_sdk_config()))
                .clone()
        }
        const _: fn() = || {
            // Compile-time assertion that function is declared at the root
            $crate::$name();
        };
    };
    ($name:ident() -> $client:ty) => {
        $crate::aws_sdk_client_provider!($name() -> $client, interceptor = $crate::interceptor::DefaultInterceptor::new());
    };
}
