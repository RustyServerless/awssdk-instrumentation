// Lambda support module — Tower layer and make_lambda_runtime! macro.

pub mod layer;
pub mod macros;

#[doc(hidden)]
pub use lambda_runtime;
#[doc(hidden)]
pub use opentelemetry_sdk;

pub use layer::OTelFaasTrigger;
