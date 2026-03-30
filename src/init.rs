//! Telemetry initialisation helpers and SDK client convenience macros.
//!
//! This module provides two levels of setup:
//!
//! - **[`default_telemetry_init()`]** — one-call setup that builds a
//!   [`SdkTracerProvider`], registers it as the global OTel provider, and
//!   (when `tracing-backend` is enabled) installs a `tracing-subscriber` with
//!   a JSON console layer and an OTel bridge layer.
//! - **[`default_tracer_provider()`]** — builds only the provider, without
//!   touching the global state. Use this when you need to compose the
//!   subscriber yourself.
//!
//! When `tracing-backend` is enabled, two additional helpers are available:
//!
//! - **[`default_tracing_otel_layer()`]** — creates the `tracing-opentelemetry`
//!   layer that bridges `tracing` spans to OTel.
//! - **[`default_tracing_console_layer()`]** — creates a JSON console layer
//!   driven by `RUST_LOG`.
//!
//! ## Sampling strategy
//!
//! The default sampler is `ParentBased(AlwaysOff)` when `env-lambda` is
//! enabled (Lambda controls sampling via the X-Ray trace header) and
//! `ParentBased(AlwaysOn)` otherwise.
//!
//! ## X-Ray annotation / metadata
//!
//! When `export-xray` is enabled, two environment variables control how span
//! attributes are mapped to X-Ray annotations and metadata:
//!
//! - `XRAY_ANNOTATIONS` — set to `"all"` to index every attribute as an
//!   annotation, or to a space-separated list of attribute keys.
//! - `XRAY_METADATA` — same format; controls which attributes go into X-Ray
//!   metadata.
//!
//! ## Convenience macros
//!
//! Two macros reduce the boilerplate of managing SDK config and client
//! singletons in Lambda functions:
//!
//! - **[`crate::aws_sdk_config_provider!`]** — declares a `OnceLock`-backed
//!   `aws_sdk_config()` function and a `sdk_config_init()` async initialiser.
//! - **[`crate::aws_sdk_client_provider!`]** — declares a `OnceLock`-backed client
//!   accessor function, optionally attaching a [`crate::interceptor::DefaultInterceptor`].
//!
//! Both macros are typically invoked indirectly through [`make_lambda_runtime!`].
//!
//! [`SdkTracerProvider`]: opentelemetry_sdk::trace::SdkTracerProvider
//! [`make_lambda_runtime!`]: crate::make_lambda_runtime

// TracerProvider builder — sensible-default initialization with support for
// user overrides (span processor, exporter, resource, propagator).

use opentelemetry::{global, trace::Tracer as OtelTracer};
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
#[cfg(feature = "tracing-backend")]
use tracing::Subscriber;
#[cfg(feature = "tracing-backend")]
use tracing_subscriber::{Layer, registry::LookupSpan};

use crate::env::default_resource;

/// Environment variable controlling which span attributes are indexed as X-Ray annotations.
///
/// Set to `"all"` to index every attribute, or to a space-separated list of attribute keys.
const ANNOTATION_ATTRIBUTES_ENV_VAR: &str = "XRAY_ANNOTATIONS";
/// Environment variable controlling which span attributes are stored as X-Ray metadata.
///
/// Set to `"all"` to include every attribute, or to a space-separated list of attribute keys.
const METADATA_ATTRIBUTES_ENV_VAR: &str = "XRAY_METADATA";

/// Default sampler: `AlwaysOff` under `env-lambda` (Lambda controls sampling via X-Ray header),
/// `AlwaysOn` otherwise.
const DEFAULT_SAMPLING_STRATEGY: Sampler = if cfg!(feature = "env-lambda") {
    Sampler::AlwaysOff
} else {
    Sampler::AlwaysOn
};

/// Initializes the global OTel telemetry stack with sensible defaults.
///
/// This function:
///
/// 1. Builds an [`SdkTracerProvider`] via [`default_tracer_provider`].
/// 2. Registers it as the global OTel provider with
///    [`opentelemetry::global::set_tracer_provider`].
/// 3. When `tracing-backend` is enabled, installs a `tracing-subscriber` with
///    a JSON console layer (driven by `RUST_LOG`) and a `tracing-opentelemetry`
///    bridge layer.
///
/// Call this once at the start of your Lambda handler's `main` function, before
/// creating any AWS SDK clients. The returned provider must be kept alive for
/// the duration of the process; pass it to the flush callback in
/// [`layer::TracingLayer`].
///
/// For more control over the subscriber stack, use [`default_tracer_provider`]
/// and compose the layers yourself with [`default_tracing_otel_layer`] and
/// [`default_tracing_console_layer`].
///
/// [`layer::TracingLayer`]: crate::lambda::layer::TracingLayer
pub fn default_telemetry_init() -> SdkTracerProvider {
    let tracer_provider = default_tracer_provider();
    global::set_tracer_provider(tracer_provider.clone());

    #[cfg(feature = "tracing-backend")]
    {
        use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

        let tracer = global::tracer(env!("CARGO_PKG_NAME"));
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

/// Builds an [`SdkTracerProvider`] with sensible defaults, without touching global state.
///
/// The provider is configured with:
///
/// - **Sampler**: `ParentBased(AlwaysOff)` when `env-lambda` is enabled (Lambda
///   controls sampling via the X-Ray trace header), or `ParentBased(AlwaysOn)`
///   otherwise.
/// - **Resource**: auto-detected via [`crate::env::default_resource`].
/// - **Exporter** (when `export-xray` is enabled): an X-Ray daemon exporter
///   with an X-Ray ID generator. The `XRAY_ANNOTATIONS` and `XRAY_METADATA`
///   environment variables control which span attributes are indexed as X-Ray
///   annotations or metadata.
///
/// Use this function instead of [`default_telemetry_init`] when you need to
/// compose the `tracing-subscriber` stack yourself.
///
/// # Examples
///
/// ```no_run
/// use awssdk_instrumentation::init::default_tracer_provider;
/// use opentelemetry::global;
///
/// let tracer_provider = default_tracer_provider();
/// global::set_tracer_provider(tracer_provider.clone());
/// ```
pub fn default_tracer_provider() -> SdkTracerProvider {
    let builder = SdkTracerProvider::builder()
        .with_sampler(Sampler::ParentBased(Box::new(DEFAULT_SAMPLING_STRATEGY)))
        .with_resource(default_resource());

    #[cfg(feature = "export-xray")]
    let builder = {
        use opentelemetry_aws::{
            trace::XrayIdGenerator,
            xray_exporter::{SegmentTranslator, XrayExporter, daemon_client::XrayDaemonClient},
        };

        let translator = SegmentTranslator::new();
        let translator = match std::env::var(ANNOTATION_ATTRIBUTES_ENV_VAR) {
            Ok(value) => {
                if value == "all" {
                    translator.index_all_attrs()
                } else {
                    translator.with_indexed_attrs(
                        value.split(" ").map(|attr_key| attr_key.trim().to_owned()),
                    )
                }
            }
            Err(_) => translator,
        };
        let translator = match std::env::var(METADATA_ATTRIBUTES_ENV_VAR) {
            Ok(value) => {
                if value == "all" {
                    translator.metadata_all_attrs()
                } else {
                    translator.with_metadata_attrs(
                        value.split(" ").map(|attr_key| attr_key.trim().to_owned()),
                    )
                }
            }
            Err(_) => translator,
        };

        builder
            .with_id_generator(XrayIdGenerator::default())
            .with_batch_exporter(
                XrayExporter::new(XrayDaemonClient::default()).with_translator(translator),
            )
    };

    builder.build()
}

/// Creates a `tracing-opentelemetry` layer that bridges `tracing` spans to OTel.
///
/// The layer is configured to forward only spans and events at `INFO` level or
/// above, plus the AWS SDK operation spans (identified by their target
/// containing `::operation::`) and the Lambda runtime layer span (target ending
/// with `::tracing_runtime_layer`). This keeps the OTel trace focused on
/// meaningful SDK and invocation spans while suppressing noisy debug events.
///
/// Pass the layer to a `tracing-subscriber` registry alongside
/// [`default_tracing_console_layer`].
///
/// # Examples
///
/// ```no_run
/// use awssdk_instrumentation::init::{default_tracer_provider, default_tracing_otel_layer};
/// use opentelemetry::global;
/// use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
///
/// let tracer_provider = default_tracer_provider();
/// global::set_tracer_provider(tracer_provider.clone());
/// let tracer = global::tracer("my-lambda");
/// let otel_layer = default_tracing_otel_layer(tracer);
///
/// tracing_subscriber::registry()
///     .with(otel_layer)
///     .init();
/// ```
#[cfg(feature = "tracing-backend")]
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

/// Creates a JSON console logging layer driven by the `RUST_LOG` environment variable.
///
/// The layer emits structured JSON log lines to stdout, without ANSI colour
/// codes and without the target field, making it suitable for CloudWatch Logs
/// ingestion. The log level filter is read from `RUST_LOG` at startup.
///
/// Pass the layer to a `tracing-subscriber` registry alongside
/// [`default_tracing_otel_layer`].
///
/// # Examples
///
/// ```no_run
/// use awssdk_instrumentation::init::{default_tracer_provider, default_tracing_console_layer};
/// use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
///
/// let console_layer = default_tracing_console_layer();
///
/// tracing_subscriber::registry()
///     .with(console_layer)
///     .init();
/// ```
#[cfg(feature = "tracing-backend")]
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

/// Declares a `OnceLock`-backed `aws_sdk_config()` accessor and an async
/// `sdk_config_init()` initialiser.
///
/// Invoke this macro once at the crate root to generate:
///
/// - A `static AWS_SDK_CONFIG: OnceLock<SdkConfig>` — the backing storage.
/// - `fn aws_sdk_config() -> &'static SdkConfig` — returns the initialized
///   config; panics if called before `sdk_config_init()`.
/// - `async fn sdk_config_init()` — loads the SDK config from the environment
///   via `aws_config::load_from_env()` and stores it in the lock.
///
/// A compile-time assertion verifies that `aws_sdk_config` is declared at the
/// crate root.
///
/// This macro is typically invoked indirectly through [`make_lambda_runtime!`]
/// when you pass client declarations to that macro.
///
/// # Examples
///
/// ```no_run
/// awssdk_instrumentation::aws_sdk_config_provider!();
///
/// #[tokio::main]
/// async fn main() {
///     sdk_config_init().await;
///     let config = aws_sdk_config();
/// }
/// ```
///
/// [`make_lambda_runtime!`]: crate::make_lambda_runtime
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

/// Declares a `OnceLock`-backed AWS SDK client accessor function.
///
/// Invoke this macro to generate a function `$name() -> $client` that returns
/// a lazily-initialized, cloned SDK client stored in a `static OnceLock`.
///
/// # Syntax
///
/// ```text
/// aws_sdk_client_provider!(fn_name() -> ClientType)
/// aws_sdk_client_provider!(fn_name() -> ClientType, interceptor = expr)
/// aws_sdk_client_provider!(fn_name() -> ClientType, no_interceptor)
/// ```
///
/// - **Default** (no suffix): attaches a [`DefaultInterceptor`] to the client
///   config automatically.
/// - **`interceptor = expr`**: attaches the provided interceptor expression.
/// - **`no_interceptor`**: creates the client without any interceptor.
///
/// The generated function reads the SDK config from `aws_sdk_config()`, which
/// must have been initialized by [`aws_sdk_config_provider!`] before the first
/// call.
///
/// A compile-time assertion verifies that the generated function is declared at
/// the crate root.
///
/// This macro is typically invoked indirectly through [`make_lambda_runtime!`]
/// when you pass client declarations to that macro.
///
/// # Examples
///
/// ```no_run
/// awssdk_instrumentation::aws_sdk_config_provider!();
/// awssdk_instrumentation::aws_sdk_client_provider!(
///     dynamodb_client() -> aws_sdk_dynamodb::Client
/// );
///
/// #[tokio::main]
/// async fn main() {
///     sdk_config_init().await;
///     let client = dynamodb_client();
/// }
/// ```
///
/// [`DefaultInterceptor`]: crate::interceptor::DefaultInterceptor
/// [`make_lambda_runtime!`]: crate::make_lambda_runtime
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
                    // NB: It is virtually guaranteed to *NOT* be included in the final binary
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
