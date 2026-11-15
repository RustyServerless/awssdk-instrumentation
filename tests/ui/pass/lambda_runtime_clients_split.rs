//! Checks that two SDK client declarations, split apart by `extra_init_code`
//! in the macro's argument list, are both collected and that both client
//! accessors are already initialized (callable without panicking) by the
//! time `extra_init_code` runs.

use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent};
use serde_json::Value;

async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    let _ = dyndb();
    let _ = dyndb2();
    Ok(event.payload)
}

awssdk_instrumentation::make_lambda_runtime!(
    handler,
    dyndb() -> aws_sdk_dynamodb::Client,
    extra_init_code = {
        let _ = dyndb();
        // Prevents a warning for unreachable code
        if true {
            return Ok(());
        }
    },
    dyndb2() -> aws_sdk_dynamodb::Client
);

#[allow(dead_code)]
fn _static_checks() {
    let _: fn() -> aws_sdk_dynamodb::Client = dyndb;
    let _: fn() -> aws_sdk_dynamodb::Client = dyndb2;
    let _: fn() -> &'static awssdk_instrumentation::aws_config::SdkConfig = aws_sdk_config;
    let _ = sdk_config_init();
}
