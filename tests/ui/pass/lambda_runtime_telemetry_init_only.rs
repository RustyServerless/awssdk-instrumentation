//! Checks `make_lambda_runtime!(handler, telemetry_init = <path>)` with a
//! qualified path (a function nested in a module) as `telemetry_init`. The
//! function exits the process immediately, which serves as the required
//! terminator for this root-level invocation (no `extra_init_code` is given).

use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent};
use serde_json::Value;

async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    Ok(event.payload)
}

mod setup {
    pub fn exit_early() -> awssdk_instrumentation::opentelemetry_sdk::trace::SdkTracerProvider {
        std::process::exit(0)
    }
}

awssdk_instrumentation::make_lambda_runtime!(
    handler,
    telemetry_init = setup::exit_early
);
