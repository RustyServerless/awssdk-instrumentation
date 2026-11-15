//! Checks `aws_sdk_client_provider!(.., no_interceptor)` generates a client
//! accessor without attaching any interceptor.
//!
//! NOTE: expected to currently fail to compile. The `no_interceptor` macro
//! arm uses `$crate::aws_sdk_config()` and `$crate::$name()`, but both
//! `aws_sdk_config` and the generated accessor live in the *invoking* crate,
//! not in `awssdk_instrumentation` itself — this is a `$crate::` bug in the
//! macro, not a mistake in this test.

awssdk_instrumentation::aws_sdk_config_provider!();
awssdk_instrumentation::aws_sdk_client_provider!(dyndb() -> aws_sdk_dynamodb::Client, no_interceptor);

fn main() {
    let _ = sdk_config_init();

    let _: fn() -> aws_sdk_dynamodb::Client = dyndb;
}
