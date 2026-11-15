#![recursion_limit = "256"]
//! Checks `make_lambda_runtime!` with two clients declared in reverse order
//! (`dyndb2` before `dyndb`) followed by `lambda_layer_instrumentor,
//! sdk_client_interceptor, trigger, telemetry_init, extra_init_code`.

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
    dyndb2() -> aws_sdk_dynamodb::Client,
    dyndb() -> aws_sdk_dynamodb::Client,
    lambda_layer_instrumentor = DefaultInstrumentor,
    sdk_client_interceptor = DefaultInterceptor::new(),
    trigger = OTelFaasTrigger::Datasource,
    telemetry_init = my_telemetry_init,
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
    let _: fn() -> aws_sdk_dynamodb::Client = dyndb2;
    let _: fn() -> &'static awssdk_instrumentation::aws_config::SdkConfig = aws_sdk_config;
    let _ = sdk_config_init();
}
