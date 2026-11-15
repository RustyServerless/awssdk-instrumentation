//! Checks `aws_sdk_client_provider!(.., no_interceptor)` generates a client
//! accessor without attaching any interceptor.

awssdk_instrumentation::aws_sdk_config_provider!();
awssdk_instrumentation::aws_sdk_client_provider!(dyndb() -> aws_sdk_dynamodb::Client, no_interceptor);

fn main() {
    let _ = sdk_config_init();

    let _: fn() -> aws_sdk_dynamodb::Client = dyndb;
}
