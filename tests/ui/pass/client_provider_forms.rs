//! Checks the three `aws_sdk_client_provider!` forms — default
//! (`DefaultInterceptor`), explicit `interceptor = expr`, and a user-defined
//! `Intercept` impl — all produce a working `OnceLock`-backed client accessor
//! at the crate root.

use aws_smithy_runtime_api::client::interceptors::Intercept;
use awssdk_instrumentation::interceptor::DefaultInterceptor;

#[derive(Debug)]
struct CustomInterceptor;

impl Intercept for CustomInterceptor {
    fn name(&self) -> &'static str {
        "CustomInterceptor"
    }
}

awssdk_instrumentation::aws_sdk_config_provider!();

awssdk_instrumentation::aws_sdk_client_provider!(a() -> aws_sdk_dynamodb::Client);
awssdk_instrumentation::aws_sdk_client_provider!(
    b() -> aws_sdk_dynamodb::Client, interceptor = DefaultInterceptor::new()
);
awssdk_instrumentation::aws_sdk_client_provider!(
    c() -> ::aws_sdk_dynamodb::Client, interceptor = CustomInterceptor
);

fn main() {
    let _ = sdk_config_init();

    let _: fn() -> aws_sdk_dynamodb::Client = a;
    let _: fn() -> aws_sdk_dynamodb::Client = b;
    let _: fn() -> aws_sdk_dynamodb::Client = c;
}
