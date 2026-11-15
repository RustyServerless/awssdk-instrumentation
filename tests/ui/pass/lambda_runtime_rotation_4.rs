//! Checks `make_lambda_runtime!` with parameters in the order
//! `sdk_client_interceptor, lambda_layer_instrumentor, client,
//! extra_init_code, telemetry_init, client, trigger` — two client
//! declarations separated by other named parameters.

use awssdk_instrumentation::interceptor::DefaultInterceptor;
use awssdk_instrumentation::lambda::layer::DefaultInstrumentor;
use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent, OTelFaasTrigger};
use awssdk_instrumentation::opentelemetry_sdk::trace::SdkTracerProvider;
use serde_json::Value;

async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    let _ = dyndb();
    let _ = dyndb2();
    Ok(event.payload)
}

fn my_telemetry_init() -> SdkTracerProvider {
    awssdk_instrumentation::init::default_telemetry_init()
}

awssdk_instrumentation::make_lambda_runtime!(
    handler,
    sdk_client_interceptor = DefaultInterceptor::new(),
    lambda_layer_instrumentor = DefaultInstrumentor,
    dyndb() -> aws_sdk_dynamodb::Client,
    extra_init_code = {
        let _ = dyndb();
        let _ = dyndb2();
        // Prevents a warning for unreachable code
        if true {
            return Ok(());
        }
    },
    telemetry_init = my_telemetry_init,
    dyndb2() -> aws_sdk_dynamodb::Client,
    trigger = OTelFaasTrigger::Timer
);

#[allow(dead_code)]
fn _static_checks() {
    let _: fn() -> aws_sdk_dynamodb::Client = dyndb;
    let _: fn() -> aws_sdk_dynamodb::Client = dyndb2;
    let _: fn() -> &'static awssdk_instrumentation::aws_config::SdkConfig = aws_sdk_config;
    let _ = sdk_config_init();
}
