//! Checks `make_lambda_runtime!` with parameters in the order
//! `extra_init_code, telemetry_init, trigger, sdk_client_interceptor,
//! lambda_layer_instrumentor, client` — named parameters may appear in any
//! order.

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
    extra_init_code = {
        // Prevents a warning for unreachable code
        if true {
            return Ok(());
        }
    },
    telemetry_init = awssdk_instrumentation::init::default_telemetry_init,
    trigger = OTelFaasTrigger::Datasource,
    sdk_client_interceptor = DefaultInterceptor::new(),
    lambda_layer_instrumentor = DefaultInstrumentor,
    dyndb() -> aws_sdk_dynamodb::Client
);

#[allow(dead_code)]
fn _static_checks() {
    let _: fn() -> aws_sdk_dynamodb::Client = dyndb;
    let _: fn() -> &'static awssdk_instrumentation::aws_config::SdkConfig = aws_sdk_config;
    let _ = sdk_config_init();
}
