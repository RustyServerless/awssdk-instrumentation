#![cfg(all(
    feature = "otel-backend",
    feature = "env-lambda",
    feature = "extract-dynamodb"
))]

//! End-to-end tests for the `otel-backend` interceptor (`OtelInterceptor`) and
//! the Lambda `TracingLayer` using `OtelInstrumentor`.
//!
//! Both backends rely on the globally registered OTel tracer provider (the
//! interceptor and the instrumentor reach it through
//! `opentelemetry::global::tracer(..)`), so this file registers one shared
//! `SdkTracerProvider` / `InMemorySpanExporter` pair once, lazily, through a
//! `OnceLock`. Every test still installs its own thread-local
//! `tracing_subscriber::registry()` default, because the interceptor always
//! needs to find the AWS SDK's own `aws_sdk_*::operation::*` tracing span,
//! regardless of the active backend.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use aws_sdk_dynamodb::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_dynamodb::types::AttributeValue;
use aws_smithy_http_client::test_util::{ReplayEvent, StaticReplayClient};
use aws_smithy_types::body::SdkBody;

use awssdk_instrumentation::interceptor::otel::OtelInterceptor;
use awssdk_instrumentation::lambda::lambda_runtime::tower::{BoxError, Layer, ServiceExt};
use awssdk_instrumentation::lambda::lambda_runtime::{
    Context as LambdaContext, LambdaInvocation, Service as TowerService, service_fn,
};
use awssdk_instrumentation::lambda::layer::{Instrumentor, OtelInstrumentor, TracingLayer};
use awssdk_instrumentation::span_write::SpanWrite;

use opentelemetry::Value;
use opentelemetry::trace::{SpanId, SpanKind, TraceId};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData};
use opentelemetry_semantic_conventions::attribute as semco;

const LAMBDA_FUNCTION_NAME: &str = "test-function";
const LAMBDA_FUNCTION_ARN: &str = "arn:aws:lambda:us-east-1:123456789012:function:test-function";

/// Shared, process-wide, lazily-registered global OTel tracer provider and
/// its in-memory exporter. `OtelInterceptor` and `OtelInstrumentor` both read
/// spans through `opentelemetry::global::tracer(..)`, so they can only be
/// exercised against a globally registered provider.
struct GlobalOtel {
    exporter: InMemorySpanExporter,
    provider: SdkTracerProvider,
}

static GLOBAL_OTEL: OnceLock<GlobalOtel> = OnceLock::new();

fn global_otel() -> &'static GlobalOtel {
    GLOBAL_OTEL.get_or_init(|| {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        opentelemetry::global::set_tracer_provider(provider.clone());
        GlobalOtel { exporter, provider }
    })
}

/// Installs a thread-local `tracing_subscriber::registry()` default. Required
/// so the interceptor can locate the AWS SDK's own `Service.Operation` span,
/// even though this backend otherwise manages OTel spans directly.
fn install_tracing_subscriber() -> tracing::subscriber::DefaultGuard {
    tracing::subscriber::set_default(tracing_subscriber::registry())
}

fn attr<'a>(span: &'a SpanData, key: &str) -> Option<&'a Value> {
    span.attributes
        .iter()
        .find(|kv| kv.key.as_str() == key)
        .map(|kv| &kv.value)
}

fn attr_str(span: &SpanData, key: &str) -> Option<String> {
    attr(span, key).map(|v| v.as_str().into_owned())
}

fn attr_i64(span: &SpanData, key: &str) -> Option<i64> {
    match attr(span, key) {
        Some(Value::I64(i)) => Some(*i),
        _ => None,
    }
}

#[allow(
    deprecated,
    reason = "rpc.system and rpc.service are set by OtelInterceptor for X-Ray compatibility"
)]
#[tokio::test]
async fn dynamodb_get_item_success_via_otel_interceptor() {
    let otel = global_otel();
    let _guard = install_tracing_subscriber();

    let response_body = r#"{"ConsumedCapacity":{"TableName":"orders","CapacityUnits":0.5}}"#;
    let replay_client = StaticReplayClient::new(vec![ReplayEvent::new(
        http::Request::builder()
            .method("POST")
            .uri("https://dynamodb.us-east-1.amazonaws.com/")
            .body(SdkBody::empty())
            .unwrap(),
        http::Response::builder()
            .status(200)
            .header("content-type", "application/x-amz-json-1.0")
            .header("x-amzn-requestid", "REQ789")
            .body(SdkBody::from(response_body))
            .unwrap(),
    )]);

    let config = aws_sdk_dynamodb::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .credentials_provider(Credentials::new("AKID", "SECRET", None, None, "test"))
        .http_client(replay_client)
        .interceptor(OtelInterceptor::new())
        .build();
    let client = aws_sdk_dynamodb::Client::from_conf(config);

    let result = client
        .get_item()
        .table_name("orders")
        .key("id", AttributeValue::S("123".to_string()))
        .send()
        .await;

    assert!(result.is_ok(), "expected success, got: {:?}", result.err());

    otel.provider.force_flush().unwrap();
    let spans = otel.exporter.get_finished_spans().unwrap();
    let span = spans
        .iter()
        .find(|s| s.name == "DynamoDB.GetItem")
        .expect("DynamoDB.GetItem span should be exported, renamed from its placeholder name");

    assert_eq!(span.span_kind, SpanKind::Client);
    assert_eq!(
        attr_str(span, semco::RPC_SERVICE).as_deref(),
        Some("DynamoDB")
    );
    assert_eq!(
        attr_str(span, semco::RPC_METHOD).as_deref(),
        Some("GetItem")
    );
    assert_eq!(
        attr_str(span, semco::RPC_SYSTEM).as_deref(),
        Some("aws-api")
    );
    assert_eq!(
        attr_str(span, semco::DB_SYSTEM_NAME).as_deref(),
        Some("aws.dynamodb")
    );
    assert!(attr(span, semco::AWS_DYNAMODB_TABLE_NAMES).is_some());
    assert_eq!(attr_i64(span, semco::HTTP_RESPONSE_STATUS_CODE), Some(200));
    assert_eq!(
        attr_str(span, semco::AWS_REQUEST_ID).as_deref(),
        Some("REQ789")
    );
}

#[tokio::test]
async fn lambda_invocation_via_otel_instrumentor() {
    let otel = global_otel();
    let _guard = install_tracing_subscriber();

    // SAFETY: every test in this file that sets `AWS_LAMBDA_FUNCTION_NAME`
    // uses this exact value, so a concurrent write from another test can
    // never observe (or produce) a different one.
    unsafe {
        std::env::set_var("AWS_LAMBDA_FUNCTION_NAME", LAMBDA_FUNCTION_NAME);
    }

    let flush_count = Arc::new(AtomicUsize::new(0));
    let flush_count_clone = Arc::clone(&flush_count);
    let layer = TracingLayer::<_, OtelInstrumentor>::new(move || {
        flush_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    async fn handler(_invocation: LambdaInvocation) -> Result<(), BoxError> {
        OtelInstrumentor::with_invocation_span(|ctx| {
            ctx.set_attribute("app.key", "value");
        });
        Ok(())
    }

    let mut service = layer.layer(service_fn(handler));

    let mut context = LambdaContext::default();
    context.request_id = "req-otel-1".to_string();
    context.invoked_function_arn = LAMBDA_FUNCTION_ARN.to_string();
    context.xray_trace_id = Some(
        "Root=1-5759e988-bd862e3fe1be46a994272793;Parent=53995c3f42cd8ad8;Sampled=1".to_string(),
    );
    let invocation = LambdaInvocation {
        parts: http::Response::new(()).into_parts().0,
        body: Default::default(),
        context,
    };

    service.ready().await.expect("service should be ready");
    service
        .call(invocation)
        .await
        .expect("invocation should succeed");

    otel.provider.force_flush().unwrap();
    let spans = otel.exporter.get_finished_spans().unwrap();
    let span = spans
        .iter()
        .find(|s| {
            s.name == "Invocation"
                && attr_str(s, semco::FAAS_INVOCATION_ID).as_deref() == Some("req-otel-1")
        })
        .expect("Invocation span for req-otel-1 should be exported");

    assert_eq!(
        attr_str(span, semco::FAAS_INVOCATION_ID).as_deref(),
        Some("req-otel-1")
    );
    assert_eq!(
        attr_str(span, semco::CLOUD_RESOURCE_ID).as_deref(),
        Some(LAMBDA_FUNCTION_ARN)
    );
    assert_eq!(
        attr_str(span, semco::SERVICE_NAME).as_deref(),
        Some(LAMBDA_FUNCTION_NAME)
    );
    assert!(attr(span, "xray_trace_id").is_some());
    assert_eq!(attr_str(span, "app.key").as_deref(), Some("value"));

    assert_eq!(
        span.span_context.trace_id(),
        TraceId::from_hex("5759e988bd862e3fe1be46a994272793").unwrap()
    );
    assert_eq!(
        span.parent_span_id,
        SpanId::from_hex("53995c3f42cd8ad8").unwrap()
    );

    assert_eq!(flush_count.load(Ordering::SeqCst), 1);
}
