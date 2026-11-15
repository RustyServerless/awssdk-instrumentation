//! `aws_sdk_config_provider!()` invoked inside a module, not at the crate
//! root — the macro's compile-time assertion (`crate::aws_sdk_config()`)
//! must fail to resolve, since `crate::` always refers to the crate root
//! regardless of lexical nesting.

mod nested {
    awssdk_instrumentation::aws_sdk_config_provider!();

    #[allow(dead_code)]
    pub fn use_it() {
        let _ = sdk_config_init();
    }
}

fn main() {}
