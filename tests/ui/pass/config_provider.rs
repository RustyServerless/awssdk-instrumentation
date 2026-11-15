//! Checks `aws_sdk_config_provider!()` declares a root-level `aws_sdk_config()`
//! accessor and an async `sdk_config_init()` initialiser with the expected
//! signatures.

use awssdk_instrumentation::aws_config::SdkConfig;

awssdk_instrumentation::aws_sdk_config_provider!();

fn assert_future<F: core::future::Future<Output = ()>>(_fut: F) {}

fn main() {
    let _: fn() -> &'static SdkConfig = aws_sdk_config;

    // Constructs the future without polling it.
    assert_future(sdk_config_init());
}
