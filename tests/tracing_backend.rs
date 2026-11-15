#![cfg(all(
    feature = "tracing-backend",
    feature = "env-lambda",
    feature = "extract-dynamodb"
))]

//! End-to-end tests for the `tracing-backend` interceptor and the Lambda
//! `TracingLayer` using `TracingInstrumentor`.
//!
//! Each test installs its own thread-local `tracing` subscriber (a
//! `tracing_subscriber::registry()` bridged to a private `SdkTracerProvider`
//! via `default_tracing_otel_layer`). No global OTel provider or global
//! subscriber is used anywhere in this file.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use aws_sdk_dynamodb::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_dynamodb::types::{AttributeValue, ReturnConsumedCapacity};
use aws_smithy_http_client::test_util::{ReplayEvent, StaticReplayClient};
use aws_smithy_runtime_api::client::interceptors::context;
use aws_smithy_types::body::SdkBody;

use awssdk_instrumentation::init::default_tracing_otel_layer;
use awssdk_instrumentation::interceptor::{
    AttributeExtractor, DefaultInterceptor, Operation, Service, ServiceFilter,
};
use awssdk_instrumentation::lambda::OTelFaasTrigger;
use awssdk_instrumentation::lambda::lambda_runtime::tower::{BoxError, Layer, ServiceExt};
use awssdk_instrumentation::lambda::lambda_runtime::{
    Context as LambdaContext, LambdaInvocation, Service as TowerService, service_fn,
};
use awssdk_instrumentation::lambda::layer::{
    DefaultInstrumentor, DefaultTracingLayer, Instrumentor,
};
use awssdk_instrumentation::span_write::SpanWrite;

use opentelemetry::trace::{SpanId, SpanKind, Status, TraceId, TracerProvider};
use opentelemetry::{Array, Value};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData};
use opentelemetry_semantic_conventions::attribute as semco;
use tracing_subscriber::layer::SubscriberExt;

/// Value set for `AWS_LAMBDA_FUNCTION_NAME` by every test in this file that
/// exercises the Lambda layer, so the shared process-wide env var never
/// disagrees between concurrently running tests.
const LAMBDA_FUNCTION_NAME: &str = "test-function";
const LAMBDA_FUNCTION_ARN: &str = "arn:aws:lambda:us-east-1:123456789012:function:test-function";

/// Builds a private `SdkTracerProvider` + `InMemorySpanExporter` pair and
/// installs it as the thread-local default `tracing` subscriber for the
/// duration of the returned guard.
fn setup_subscriber() -> (
    InMemorySpanExporter,
    SdkTracerProvider,
    tracing::subscriber::DefaultGuard,
) {
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let tracer = provider.tracer("test");
    let subscriber = tracing_subscriber::registry().with(default_tracing_otel_layer(tracer));
    let guard = tracing::subscriber::set_default(subscriber);
    (exporter, provider, guard)
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

fn attr_bool(span: &SpanData, key: &str) -> Option<bool> {
    match attr(span, key) {
        Some(Value::Bool(b)) => Some(*b),
        _ => None,
    }
}

fn attr_i64(span: &SpanData, key: &str) -> Option<i64> {
    match attr(span, key) {
        Some(Value::I64(i)) => Some(*i),
        _ => None,
    }
}

fn attr_contains_str(span: &SpanData, key: &str, needle: &str) -> bool {
    matches!(
        attr(span, key),
        Some(Value::Array(Array::String(values))) if values.iter().any(|v| v.as_ref() == needle)
    )
}

fn find_span<'a>(spans: &'a [SpanData], name: &str) -> &'a SpanData {
    spans
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("span {name:?} was not exported"))
}

#[tokio::test]
async fn dynamodb_get_item_success_via_tracing_interceptor() {
    let (exporter, provider, _guard) = setup_subscriber();

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
            .header("x-amzn-requestid", "REQ123")
            .body(SdkBody::from(response_body))
            .unwrap(),
    )]);

    let config = aws_sdk_dynamodb::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .credentials_provider(Credentials::new("AKID", "SECRET", None, None, "test"))
        .http_client(replay_client)
        .interceptor(DefaultInterceptor::new())
        .build();
    let client = aws_sdk_dynamodb::Client::from_conf(config);

    let result = client
        .get_item()
        .table_name("orders")
        .key("id", AttributeValue::S("123".to_string()))
        .return_consumed_capacity(ReturnConsumedCapacity::Total)
        .send()
        .await;

    assert!(result.is_ok(), "expected success, got: {:?}", result.err());

    provider.force_flush().unwrap();
    let spans = exporter.get_finished_spans().unwrap();
    let span = find_span(&spans, "DynamoDB.GetItem");

    assert_eq!(span.span_kind, SpanKind::Internal);
    assert_eq!(
        attr_str(span, semco::RPC_SYSTEM_NAME).as_deref(),
        Some("aws-api")
    );
    assert_eq!(
        attr_str(span, semco::CLOUD_REGION).as_deref(),
        Some("us-east-1")
    );
    assert_eq!(
        attr_str(span, semco::DB_SYSTEM_NAME).as_deref(),
        Some("aws.dynamodb")
    );
    assert!(attr_contains_str(
        span,
        semco::AWS_DYNAMODB_TABLE_NAMES,
        "orders"
    ));
    assert_eq!(attr_i64(span, semco::HTTP_RESPONSE_STATUS_CODE), Some(200));
    assert_eq!(
        attr_str(span, semco::AWS_REQUEST_ID).as_deref(),
        Some("REQ123")
    );
    assert!(attr(span, semco::AWS_DYNAMODB_CONSUMED_CAPACITY).is_some());
}

#[tokio::test]
async fn dynamodb_put_item_modeled_error_and_user_extensions() {
    let (exporter, provider, _guard) = setup_subscriber();

    let response_body = r#"{"__type":"com.amazonaws.dynamodb.v20120810#ConditionalCheckFailedException","message":"The conditional request failed"}"#;
    let replay_client = StaticReplayClient::new(vec![ReplayEvent::new(
        http::Request::builder()
            .method("POST")
            .uri("https://dynamodb.us-east-1.amazonaws.com/")
            .body(SdkBody::empty())
            .unwrap(),
        http::Response::builder()
            .status(400)
            .header("content-type", "application/x-amz-json-1.0")
            .header("x-amzn-requestid", "REQ456")
            .body(SdkBody::from(response_body))
            .unwrap(),
    )]);

    struct CustomExtractor;
    impl<SW: SpanWrite> AttributeExtractor<SW> for CustomExtractor {
        fn extract_input(
            &self,
            _service: Service,
            _operation: Operation,
            _input: &context::Input,
            span: &mut SW,
        ) {
            span.set_attribute("test.custom", true);
        }
    }

    let mut interceptor = DefaultInterceptor::new();
    interceptor.extractor.register_input_hook(
        ServiceFilter::Operation("DynamoDB", "PutItem"),
        |_service, _operation, _input, span: &mut tracing::Span| {
            span.set_attribute("test.input_hook", true);
        },
    );
    interceptor.extractor.register_input_hook(
        ServiceFilter::Service("S3"),
        |_service, _operation, _input, span: &mut tracing::Span| {
            span.set_attribute("test.s3_hook", true);
        },
    );
    interceptor.extractor.register_error_hook(
        ServiceFilter::All,
        |_service, _operation, _error, span: &mut tracing::Span| {
            span.set_attribute("test.error_hook", true);
        },
    );
    interceptor
        .extractor
        .register_attribute_extractor(CustomExtractor);

    let config = aws_sdk_dynamodb::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .credentials_provider(Credentials::new("AKID", "SECRET", None, None, "test"))
        .http_client(replay_client)
        .interceptor(interceptor)
        .build();
    let client = aws_sdk_dynamodb::Client::from_conf(config);

    let result = client
        .put_item()
        .table_name("orders")
        .item("id", AttributeValue::S("123".to_string()))
        .send()
        .await;

    assert!(result.is_err());

    provider.force_flush().unwrap();
    let spans = exporter.get_finished_spans().unwrap();
    let span = find_span(&spans, "DynamoDB.PutItem");

    assert_eq!(
        attr_str(span, semco::ERROR_TYPE).as_deref(),
        Some("ConditionalCheckFailedException")
    );
    match &span.status {
        Status::Error { description } => {
            assert!(
                description.contains("conditional request failed"),
                "unexpected status description: {description}"
            );
        }
        other => panic!("expected an error status, got {other:?}"),
    }
    assert!(attr(span, "test.input_hook").is_some());
    assert!(attr(span, "test.error_hook").is_some());
    assert!(attr(span, "test.custom").is_some());
    assert!(attr(span, "test.s3_hook").is_none());
}

#[tokio::test]
async fn lambda_invocations_through_default_tracing_layer() {
    let (exporter, provider, _guard) = setup_subscriber();

    // SAFETY: this test does not read `AWS_LAMBDA_FUNCTION_NAME` concurrently
    // with a write of a different value; every test in this file that sets it
    // uses the same value.
    unsafe {
        std::env::set_var("AWS_LAMBDA_FUNCTION_NAME", LAMBDA_FUNCTION_NAME);
    }

    let flush_count = Arc::new(AtomicUsize::new(0));
    let flush_count_clone = Arc::clone(&flush_count);
    let layer = DefaultTracingLayer::new(move || {
        flush_count_clone.fetch_add(1, Ordering::SeqCst);
    })
    .with_trigger(OTelFaasTrigger::PubSub);

    async fn handler(_invocation: LambdaInvocation) -> Result<(), BoxError> {
        DefaultInstrumentor::with_invocation_span(|span| {
            span.set_attribute("app.key", "value");
        });
        DefaultInstrumentor::spawn(async {})
            .await
            .expect("spawned task should complete");
        Ok(())
    }

    let mut service = layer.layer(service_fn(handler));

    let mut context1 = LambdaContext::default();
    context1.request_id = "req-1".to_string();
    context1.invoked_function_arn = LAMBDA_FUNCTION_ARN.to_string();
    context1.xray_trace_id = Some(
        "Root=1-5759e988-bd862e3fe1be46a994272793;Parent=53995c3f42cd8ad8;Sampled=1".to_string(),
    );
    let invocation1 = LambdaInvocation {
        parts: http::Response::new(()).into_parts().0,
        body: Default::default(),
        context: context1,
    };

    service.ready().await.expect("service should be ready");
    service
        .call(invocation1)
        .await
        .expect("first invocation should succeed");

    let mut context2 = LambdaContext::default();
    context2.request_id = "req-2".to_string();
    context2.invoked_function_arn = LAMBDA_FUNCTION_ARN.to_string();
    let invocation2 = LambdaInvocation {
        parts: http::Response::new(()).into_parts().0,
        body: Default::default(),
        context: context2,
    };

    service.ready().await.expect("service should be ready");
    service
        .call(invocation2)
        .await
        .expect("second invocation should succeed");

    provider.force_flush().unwrap();
    let spans = exporter.get_finished_spans().unwrap();
    let invocation_spans: Vec<&SpanData> =
        spans.iter().filter(|s| s.name == "Invocation").collect();
    assert_eq!(invocation_spans.len(), 2);

    let span1 = *invocation_spans
        .iter()
        .find(|s| attr_str(s, semco::FAAS_INVOCATION_ID).as_deref() == Some("req-1"))
        .expect("span for req-1 should be exported");
    let span2 = *invocation_spans
        .iter()
        .find(|s| attr_str(s, semco::FAAS_INVOCATION_ID).as_deref() == Some("req-2"))
        .expect("span for req-2 should be exported");

    assert_eq!(attr_bool(span1, semco::FAAS_COLDSTART), Some(true));
    assert_eq!(attr_bool(span2, semco::FAAS_COLDSTART), Some(false));

    assert_eq!(
        attr_str(span1, semco::FAAS_TRIGGER).as_deref(),
        Some("pubsub")
    );
    assert_eq!(
        attr_str(span2, semco::FAAS_TRIGGER).as_deref(),
        Some("pubsub")
    );

    assert_eq!(
        attr_str(span1, semco::CLOUD_RESOURCE_ID).as_deref(),
        Some(LAMBDA_FUNCTION_ARN)
    );
    assert_eq!(
        attr_str(span2, semco::CLOUD_RESOURCE_ID).as_deref(),
        Some(LAMBDA_FUNCTION_ARN)
    );

    assert_eq!(
        attr_str(span1, semco::SERVICE_NAME).as_deref(),
        Some(LAMBDA_FUNCTION_NAME)
    );
    assert_eq!(
        attr_str(span2, semco::SERVICE_NAME).as_deref(),
        Some(LAMBDA_FUNCTION_NAME)
    );

    assert_eq!(
        span1.span_context.trace_id(),
        TraceId::from_hex("5759e988bd862e3fe1be46a994272793").unwrap()
    );
    assert_eq!(
        span1.parent_span_id,
        SpanId::from_hex("53995c3f42cd8ad8").unwrap()
    );
    assert!(attr(span1, "xray_trace_id").is_some());

    assert_eq!(attr_str(span1, "app.key").as_deref(), Some("value"));
    assert_eq!(attr_str(span2, "app.key").as_deref(), Some("value"));

    let expected_kind = if cfg!(feature = "xray-no-lambda-node-nesting") {
        SpanKind::Server
    } else {
        SpanKind::Internal
    };
    assert_eq!(span1.span_kind, expected_kind);
    assert_eq!(span2.span_kind, expected_kind);

    assert_eq!(flush_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn cancelled_invocation_still_flushes() {
    let (exporter, provider, _guard) = setup_subscriber();

    // SAFETY: see the comment in `lambda_invocations_through_default_tracing_layer`.
    unsafe {
        std::env::set_var("AWS_LAMBDA_FUNCTION_NAME", LAMBDA_FUNCTION_NAME);
    }

    let flush_count = Arc::new(AtomicUsize::new(0));
    let flush_count_clone = Arc::clone(&flush_count);
    let layer = DefaultTracingLayer::new(move || {
        flush_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    let inner = service_fn(|_: LambdaInvocation| std::future::pending::<Result<(), BoxError>>());
    let mut service = layer.layer(inner);

    let mut context = LambdaContext::default();
    context.request_id = "req-cancel".to_string();
    context.invoked_function_arn = LAMBDA_FUNCTION_ARN.to_string();
    let invocation = LambdaInvocation {
        parts: http::Response::new(()).into_parts().0,
        body: Default::default(),
        context,
    };

    service.ready().await.expect("service should be ready");

    {
        let future = service.call(invocation);
        let mut future = std::pin::pin!(future);
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        let poll = future.as_mut().poll(&mut cx);
        assert!(matches!(poll, std::task::Poll::Pending));
        // `future` is dropped here, at the end of this block, cancelling the
        // invocation before it ever completes.
    }

    provider.force_flush().unwrap();
    let spans = exporter.get_finished_spans().unwrap();
    assert!(spans.iter().any(|s| s.name == "Invocation"));
    assert_eq!(flush_count.load(Ordering::SeqCst), 1);
}
