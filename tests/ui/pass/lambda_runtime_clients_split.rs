//! Checks that two SDK client declarations, split apart by `extra_init_code`
//! in the macro's argument list, are both collected and that both clients are
//! built by the generated `main` before `extra_init_code` runs. The
//! interceptor expression is evaluated once per client construction, so
//! `BUILT` counts constructed clients.

use std::sync::atomic::{AtomicUsize, Ordering};

use aws_smithy_runtime_api::client::interceptors::Intercept;
use awssdk_instrumentation::lambda::{LambdaError, LambdaEvent};
use serde_json::Value;

static BUILT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
struct CountingInterceptor;

impl CountingInterceptor {
    fn new() -> Self {
        BUILT.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Intercept for CountingInterceptor {
    fn name(&self) -> &'static str {
        "CountingInterceptor"
    }
}

async fn handler(event: LambdaEvent<Value>) -> Result<Value, LambdaError> {
    let _ = dyndb();
    let _ = dyndb2();
    Ok(event.payload)
}

awssdk_instrumentation::make_lambda_runtime!(
    handler,
    sdk_client_interceptor = CountingInterceptor::new(),
    dyndb() -> aws_sdk_dynamodb::Client,
    extra_init_code = {
        assert_eq!(BUILT.load(Ordering::SeqCst), 2);
        // Prevents a warning for unreachable code
        if true {
            return Ok(());
        }
    },
    dyndb2() -> aws_sdk_dynamodb::Client
);

#[allow(dead_code)]
fn _static_checks() {
    let _: fn() -> aws_sdk_dynamodb::Client = dyndb;
    let _: fn() -> aws_sdk_dynamodb::Client = dyndb2;
    let _: fn() -> &'static awssdk_instrumentation::aws_config::SdkConfig = aws_sdk_config;
    let _ = sdk_config_init();
}
