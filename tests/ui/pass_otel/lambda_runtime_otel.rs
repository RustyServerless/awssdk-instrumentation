//! Checks `make_lambda_runtime!` with the `otel-backend`-specific interceptor
//! and instrumentor explicitly selected via `sdk_client_interceptor` and
//! `lambda_layer_instrumentor`.

use awssdk_instrumentation::interceptor::otel::OtelInterceptor;
use awssdk_instrumentation::lambda::layer::OtelInstrumentor;
use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent};
use serde_json::Value;

async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    let _ = dyndb();
    Ok(event.payload)
}

awssdk_instrumentation::make_lambda_runtime!(
    handler,
    sdk_client_interceptor = OtelInterceptor::new(),
    lambda_layer_instrumentor = OtelInstrumentor,
    dyndb() -> aws_sdk_dynamodb::Client,
    extra_init_code = {
        // Prevents a warning for unreachable code
        if true {
            return Ok(());
        }
    }
);

#[allow(dead_code)]
fn _static_checks() {
    let _: fn() -> aws_sdk_dynamodb::Client = dyndb;
    let _: fn() -> &'static awssdk_instrumentation::aws_config::SdkConfig = aws_sdk_config;
    let _ = sdk_config_init();
}
