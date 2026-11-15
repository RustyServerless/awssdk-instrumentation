//! Checks `make_lambda_runtime!` with parameters in the order
//! `client, telemetry_init, trigger, sdk_client_interceptor,
//! lambda_layer_instrumentor, extra_init_code`. `telemetry_init` is a
//! user-defined function wrapping `default_telemetry_init`.

use awssdk_instrumentation::interceptor::DefaultInterceptor;
use awssdk_instrumentation::lambda::layer::DefaultInstrumentor;
use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent, OTelFaasTrigger};
use awssdk_instrumentation::opentelemetry_sdk::trace::SdkTracerProvider;
use serde_json::Value;

async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    let _ = dyndb();
    Ok(event.payload)
}

fn my_telemetry_init() -> SdkTracerProvider {
    awssdk_instrumentation::init::default_telemetry_init()
}

awssdk_instrumentation::make_lambda_runtime!(
    handler,
    dyndb() -> aws_sdk_dynamodb::Client,
    telemetry_init = my_telemetry_init,
    trigger = OTelFaasTrigger::Http,
    sdk_client_interceptor = DefaultInterceptor::new(),
    lambda_layer_instrumentor = DefaultInstrumentor,
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
