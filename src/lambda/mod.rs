// Lambda support module — Tower layer and make_lambda_runtime! macro.

pub mod layer;
pub mod macros;

pub use lambda_runtime;
pub use opentelemetry_sdk;

pub use layer::OTelFaasTrigger;
