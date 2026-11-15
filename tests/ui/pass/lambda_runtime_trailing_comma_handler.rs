//! Checks `make_lambda_runtime!(handler,)` — a trailing comma directly after
//! the handler with nothing else following.
//!
//! NOTE: expected to currently fail to compile. The macro's entry-point arm
//! is `($handler:path $(,$($rest:tt)+)?)`, which requires at least one token
//! after a comma; a bare trailing comma with no following tokens matches
//! neither repetition of the optional group, so the macro invocation itself
//! fails to parse. This is a limitation of the macro, not a mistake in this
//! test.

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
