//! Trybuild compile tests for `aws_sdk_config_provider!`, `aws_sdk_client_provider!`,
//! and `make_lambda_runtime!`.

#[test]
#[cfg(all(feature = "env-lambda", feature = "extract-dynamodb"))]
fn macros_compile() {
    // SAFETY: set before trybuild spawns any `cargo build` subprocess, and
    // this is the only test in the process touching these variables.
    unsafe {
        std::env::set_var("AWS_REGION", "us-east-1");
        std::env::set_var("AWS_EC2_METADATA_DISABLED", "true");
        std::env::set_var("CARGO_PROFILE_DEV_DEBUG", "0");
    }

    let t = trybuild::TestCases::new();

    t.pass("tests/ui/pass/*.rs");
    #[cfg(feature = "tracing-backend")]
    t.pass("tests/ui/pass_tracing/*.rs");
    #[cfg(feature = "otel-backend")]
    t.pass("tests/ui/pass_otel/*.rs");

    t.compile_fail("tests/ui/fail/*.rs");
}
