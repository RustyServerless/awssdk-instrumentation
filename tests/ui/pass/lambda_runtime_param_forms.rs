//! Checks unusual-but-valid syntactic forms for each `make_lambda_runtime!`
//! parameter: a qualified handler path, a trigger expression (function call),
//! a telemetry init function wrapping `default_tracer_provider()`, a
//! struct-literal interceptor with multiple fields, a locally aliased
//! instrumentor type, a client type with a leading `::`, and
//! `extra_init_code` containing commas and identifiers that shadow the
//! macro's own parameter names.

use aws_smithy_runtime_api::client::interceptors::Intercept;
use awssdk_instrumentation::lambda::layer::DefaultInstrumentor;
use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent, OTelFaasTrigger};
use awssdk_instrumentation::opentelemetry_sdk::trace::SdkTracerProvider;
use serde_json::Value;

pub mod handlers {
    use super::*;

    pub async fn handle(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
        let _ = dyndb();
        Ok(event.payload)
    }
}

#[derive(Debug)]
struct FieldsInterceptor {
    #[allow(dead_code)]
    a: u32,
    #[allow(dead_code)]
    b: u32,
}

impl Intercept for FieldsInterceptor {
    fn name(&self) -> &'static str {
        "FieldsInterceptor"
    }
}

fn pick_trigger() -> OTelFaasTrigger {
    OTelFaasTrigger::PubSub
}

fn my_telemetry_init() -> SdkTracerProvider {
    awssdk_instrumentation::init::default_tracer_provider()
}

type LocalInstrumentor = DefaultInstrumentor;

awssdk_instrumentation::make_lambda_runtime!(
    handlers::handle,
    trigger = pick_trigger(),
    telemetry_init = my_telemetry_init,
    sdk_client_interceptor = FieldsInterceptor { a: 1, b: 2 },
    lambda_layer_instrumentor = LocalInstrumentor,
    dyndb() -> ::aws_sdk_dynamodb::Client,
    extra_init_code = {
        let trigger = (1, 2);
        let telemetry_init = [trigger.0, trigger.1];
        let _ = telemetry_init;
        // Prevents a warning for unreachable code
        if true {
            return Ok(());
        }
    }
);

#[allow(dead_code)]
fn _static_checks() {
    let _: fn() -> ::aws_sdk_dynamodb::Client = dyndb;
    let _: fn() -> &'static awssdk_instrumentation::aws_config::SdkConfig = aws_sdk_config;
    let _ = sdk_config_init();
}
