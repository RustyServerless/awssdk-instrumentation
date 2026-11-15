//! `make_lambda_runtime!(handler, extra_init_code = { .. })` without any
//! client declarations does not generate `aws_sdk_config_provider!()` — a
//! call to `aws_sdk_config()` elsewhere in the crate must fail to resolve.

use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent};
use serde_json::Value;

async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    Ok(event.payload)
}

#[allow(dead_code)]
fn uses_config() {
    let _ = aws_sdk_config();
}

awssdk_instrumentation::make_lambda_runtime!(handler);
