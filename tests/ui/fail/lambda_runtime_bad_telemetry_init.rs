//! `make_lambda_runtime!(.., telemetry_init = bad_init, ..)` where `bad_init`
//! has signature `fn() -> ()` instead of the required
//! `fn() -> SdkTracerProvider` — the macro's compile-time function-pointer
//! coercion assertion must fail with a type mismatch.

use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent};
use serde_json::Value;

async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    Ok(event.payload)
}

fn bad_init() {}

awssdk_instrumentation::make_lambda_runtime!(handler, telemetry_init = bad_init,);
