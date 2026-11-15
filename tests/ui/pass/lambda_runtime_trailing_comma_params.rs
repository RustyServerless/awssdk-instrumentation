//! Checks `make_lambda_runtime!` with a user-supplied trailing comma after
//! the last named parameter (`trigger`, a client declaration, and
//! `extra_init_code`, followed by an explicit trailing comma).
//!
//! NOTE: expected to currently fail to compile. The entry-point arm always
//! appends its own trailing comma (`$($rest)+,`) after the full argument
//! list, so a user-supplied trailing comma produces a doubled comma that the
//! internal processing arms cannot match. This is a limitation of the macro,
//! not a mistake in this test.

use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent, OTelFaasTrigger};
use serde_json::Value;

async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    let _ = dyndb();
    Ok(event.payload)
}

awssdk_instrumentation::make_lambda_runtime!(
    handler,
    trigger = OTelFaasTrigger::Datasource,
    dyndb() -> aws_sdk_dynamodb::Client,
    extra_init_code = {
        let _ = dyndb();
        // Prevents a warning for unreachable code
        if true {
            return Ok(());
        }
    },
);
