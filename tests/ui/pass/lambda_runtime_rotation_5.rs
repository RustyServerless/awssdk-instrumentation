//! Checks `make_lambda_runtime!` with parameters in the order
//! `lambda_layer_instrumentor, extra_init_code, telemetry_init, trigger,
//! client, sdk_client_interceptor`.

use awssdk_instrumentation::interceptor::DefaultInterceptor;
use awssdk_instrumentation::lambda::layer::DefaultInstrumentor;
use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent, OTelFaasTrigger};
use serde_json::Value;

async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    let _ = dyndb();
    Ok(event.payload)
}

awssdk_instrumentation::make_lambda_runtime!(
    handler,
    lambda_layer_instrumentor = DefaultInstrumentor,
    extra_init_code = {
        let _ = dyndb();
        // Prevents a warning for unreachable code
        if true {
            return Ok(());
        }
    },
    telemetry_init = awssdk_instrumentation::init::default_telemetry_init,
    trigger = OTelFaasTrigger::Other,
    dyndb() -> aws_sdk_dynamodb::Client,
    sdk_client_interceptor = DefaultInterceptor::new()
);

#[allow(dead_code)]
fn _static_checks() {
    let _: fn() -> aws_sdk_dynamodb::Client = dyndb;
    let _: fn() -> &'static awssdk_instrumentation::aws_config::SdkConfig = aws_sdk_config;
    let _ = sdk_config_init();
}
