//! Checks `make_lambda_runtime!(handler, extra_init_code = { .. })` at the
//! crate root: only `handler` and `extra_init_code` are given, every other
//! parameter defaults. `extra_init_code` returns early so the generated
//! `main` never reaches `Runtime::run()`.

use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent};
use serde_json::Value;

async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    Ok(event.payload)
}

awssdk_instrumentation::make_lambda_runtime!(
    handler,
    extra_init_code = {
        // Prevents a warning for unreachable code
        if true {
            return Ok(());
        }
    }
);
