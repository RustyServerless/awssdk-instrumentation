//! Checks `make_lambda_runtime!(handler)` with only the required `handler`
//! argument, invoked from a nested module. No clients are declared, so
//! `aws_sdk_config_provider!` is never invoked and the crate-root compile-time
//! assertion does not apply — the generated `main` can safely live outside
//! the crate root.

use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent};
use serde_json::Value;

mod lambda {
    use super::*;

    async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
        Ok(event.payload)
    }

    awssdk_instrumentation::make_lambda_runtime!(handler);
}

fn main() {}
