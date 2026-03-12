// Crate root — re-exports and feature-gated module declarations.

#[cfg(not(any(feature = "otel-backend", feature = "tracing-backend")))]
compile_error!("At least one of \"otel-backend\" or \"tracing-backend\" features must be enabled");

pub mod interceptor;

#[cfg(feature = "env-lambda")]
pub mod lambda;

pub mod env;
pub mod utils;

pub mod init;

#[cfg(feature = "export-xray")]
pub use opentelemetry_aws;
