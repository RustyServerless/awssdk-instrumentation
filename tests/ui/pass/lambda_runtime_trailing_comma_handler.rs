//! Checks `make_lambda_runtime!(handler,)` — a trailing comma directly after
//! the handler with nothing else following.

use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent};
use serde_json::Value;

mod lambda {
    use super::*;

    async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
        Ok(event.payload)
    }

    awssdk_instrumentation::make_lambda_runtime!(handler,);
}

fn main() {}
