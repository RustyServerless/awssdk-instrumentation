//! Checks `make_lambda_runtime!` with every named parameter except client
//! declarations: `trigger, sdk_client_interceptor, extra_init_code,
//! lambda_layer_instrumentor, telemetry_init`. No `aws_sdk_config_provider!`
//! is generated since no client is declared, so `aws_sdk_config` /
//! `sdk_config_init` are not expected to exist in this file.

use awssdk_instrumentation::lambda::layer::DefaultInstrumentor;
use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent, OTelFaasTrigger};
use serde_json::Value;

async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    Ok(event.payload)
}

awssdk_instrumentation::make_lambda_runtime!(
    handler,
    trigger = OTelFaasTrigger::Http,
    sdk_client_interceptor = DefaultInterceptor::new(),
    extra_init_code = {
        // Prevents a warning for unreachable code
        if true {
            return Ok(());
        }
    },
    lambda_layer_instrumentor = DefaultInstrumentor,
    telemetry_init = awssdk_instrumentation::init::default_telemetry_init
);
