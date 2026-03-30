//! AWS SDK interceptor that automatically extracts OTel semantic-convention
//! attributes from SDK calls.
//!
//! The central type is [`DefaultInterceptor`], a type alias that resolves to
//! [`tracing::TracingInterceptor`] or [`otel::OtelInterceptor`] depending on
//! the active backend feature. Attach it to any AWS SDK client config and every
//! SDK call will produce a span enriched with service, operation, region,
//! request ID, HTTP status, and service-specific attributes.
//!
//! # Attribute extraction pipeline
//!
//! For each SDK call the interceptor runs four extraction phases in order:
//!
//! 1. **Input** — before serialization; extracts table names, bucket names, etc.
//! 2. **Request** — after serialization; extracts HTTP-level request attributes.
//! 3. **Response** — before deserialization; extracts HTTP status, request ID,
//!    and extended request ID.
//! 4. **Output / Error** — after execution; extracts consumed capacity, message
//!    IDs, error codes, and sets the span status.
//!
//! Within each phase the dispatch order is:
//!
//! 1. Built-in per-service extractors (DynamoDB, S3, SQS — feature-gated).
//! 2. User-registered [`AttributeExtractor`] implementations.
//! 3. User-registered closure hooks, filtered by [`ServiceFilter`].
//!
//! # Extending extraction
//!
//! Access the [`DefaultExtractor`] inside the interceptor to register hooks:
//!
//! ```no_run
//! use awssdk_instrumentation::interceptor::{
//!     DefaultInterceptor, ServiceFilter,
//! };
//!
//! let mut interceptor = DefaultInterceptor::new();
//!
//! // Log the table name for every DynamoDB call.
//! interceptor.extractor.register_input_hook(
//!     ServiceFilter::Service("DynamoDB"),
//!     |service, operation, _input, _span| {
//!         println!("DynamoDB call: {service}.{operation}");
//!     },
//! );
//! ```
//!
//! For structured extraction logic implement [`AttributeExtractor`] and register
//! it with [`DefaultExtractor::register_attribute_extractor`].

// Interceptor module — AttributeExtractor trait, DefaultExtractor, ServiceFilter,
// closure registration, and service dispatch logic.

// Semantic convention constants not yet available in the `opentelemetry-semantic-conventions`
// crate. Named identically to the upstream pattern so the compiler will error once the
// crate exports them and we can remove these definitions.
/// OpenTelemetry semantic convention key for the database system name (`db.system.name`).
///
/// Used by service extractors (e.g. [`extract::dynamodb::DynamoDBExtractor`]) to tag spans
/// with the database technology. Set to `"aws.dynamodb"` for DynamoDB calls.
///
/// This constant is defined locally because the upstream
/// `opentelemetry-semantic-conventions` crate does not yet export it. Once the
/// upstream crate adds it, this definition will produce a compile error and can
/// be removed.
pub const DB_SYSTEM_NAME: &str = "db.system.name";

/// OpenTelemetry semantic convention key for the RPC system name (`rpc.system.name`).
///
/// Used by the OTel-native backend interceptor to tag spans with the RPC technology.
/// Set to `"aws-api"` for all AWS SDK calls.
///
/// This constant is defined locally because the upstream
/// `opentelemetry-semantic-conventions` crate does not yet export it. Once the
/// upstream crate adds it, this definition will produce a compile error and can
/// be removed.
pub const RPC_SYSTEM_NAME: &str = "rpc.system.name";
/// Compile-time sentinel that will error when `opentelemetry-semantic-conventions` exports
/// [`DB_SYSTEM_NAME`] or [`RPC_SYSTEM_NAME`], signalling that the local definitions can be removed.
#[allow(unused)]
mod _tell_me_when_semconv_have_it {
    use super::{DB_SYSTEM_NAME, RPC_SYSTEM_NAME};
    use opentelemetry_semantic_conventions::attribute::*;
}

pub mod extract;
mod utils;

#[cfg(feature = "tracing-backend")]
pub mod tracing;

#[cfg(feature = "otel-backend")]
pub mod otel;

/// The default AWS SDK interceptor for the active backend.
///
/// This type alias resolves to [`tracing::TracingInterceptor`] when the
/// `tracing-backend` feature is enabled, or to [`otel::OtelInterceptor`] when
/// only `otel-backend` is active. Using `DefaultInterceptor` in your code lets
/// you switch backends by changing feature flags without touching call sites.
///
/// Attach the interceptor to an AWS SDK client config to automatically enrich
/// every SDK call with OTel semantic-convention attributes (region, operation,
/// HTTP status, request ID, and service-specific attributes).
///
/// # Examples
///
/// ```no_run
/// # async fn example() {
/// use awssdk_instrumentation::interceptor::DefaultInterceptor;
///
/// let interceptor = DefaultInterceptor::new();
/// // Attach to an AWS SDK config builder, e.g.:
/// let config = aws_config::load_from_env().await;
/// let dynamo_config = aws_sdk_dynamodb::config::Builder::from(&config)
///     .interceptor(interceptor)
///     .build();
/// # }
/// ```
#[cfg(feature = "tracing-backend")]
pub type DefaultInterceptor = tracing::TracingInterceptor;

/// The default AWS SDK interceptor for the active backend.
///
/// This type alias resolves to [`otel::OtelInterceptor`] when only the
/// `otel-backend` feature is active, or to [`tracing::TracingInterceptor`] when
/// `tracing-backend` is enabled. Using `DefaultInterceptor` in your code lets
/// you switch backends by changing feature flags without touching call sites.
///
/// Attach the interceptor to an AWS SDK client config to automatically enrich
/// every SDK call with OTel semantic-convention attributes (region, operation,
/// HTTP status, request ID, and service-specific attributes).
///
/// # Examples
///
/// ```no_run
/// use awssdk_instrumentation::interceptor::DefaultInterceptor;
///
/// let interceptor = DefaultInterceptor::new();
/// // Attach to an AWS SDK config builder, e.g.:
/// let config = aws_config::load_from_env().await;
/// let dynamo_config = aws_sdk_dynamodb::config::Builder::from(&config)
///     .interceptor(interceptor)
///     .build();
/// ```
#[cfg(all(feature = "otel-backend", not(feature = "tracing-backend")))]
pub type DefaultInterceptor = otel::OtelInterceptor;

use aws_smithy_runtime_api::{box_error::BoxError, client::interceptors::context, http};
use aws_smithy_types::config_bag::ConfigBag;
use aws_types::{region::Region, request_id::RequestId};

use opentelemetry::trace::Status;
use opentelemetry_semantic_conventions::attribute as semco;

use utils::{AwsSdkOperation, extract_service_operation};

use crate::span_write::SpanWrite;

/// A borrowed AWS service name, such as `"DynamoDB"` or `"S3"`.
///
/// Service names match the names used by the AWS SDK internally (e.g. the
/// `Service` segment of a `Service.Operation` span name). Comparisons performed
/// by [`ServiceFilter`] are case-insensitive.
pub type Service<'a> = &'a str;

/// A borrowed AWS operation name, such as `"GetItem"` or `"PutObject"`.
///
/// Operation names match the names used by the AWS SDK internally (e.g. the
/// `Operation` segment of a `Service.Operation` span name). Comparisons
/// performed by [`ServiceFilter`] are case-insensitive.
pub type Operation<'a> = &'a str;

/// Scope filter that controls which SDK calls trigger a registered hook or extractor.
///
/// Pass a `ServiceFilter` when registering a closure with
/// [`DefaultExtractor::register_input_hook`] and the other `register_*_hook`
/// methods. The filter is evaluated for every SDK call; the hook runs only when
/// the filter matches.
///
/// Comparisons are **case-insensitive** for both service and operation names.
///
/// # Examples
///
/// ```
/// use awssdk_instrumentation::interceptor::ServiceFilter;
///
/// // Matches every SDK call.
/// let all = ServiceFilter::All;
///
/// // Matches any DynamoDB operation.
/// let dynamo = ServiceFilter::Service("DynamoDB");
///
/// // Matches only DynamoDB GetItem calls.
/// let get_item = ServiceFilter::Operation("DynamoDB", "GetItem");
/// ```
pub enum ServiceFilter {
    /// Matches every service and operation.
    All,
    /// Matches all operations for a specific service (e.g. `"DynamoDB"`).
    Service(Service<'static>),
    /// Matches a specific operation on a specific service
    /// (e.g. `"DynamoDB"`, `"GetItem"`).
    Operation(Service<'static>, Operation<'static>),
}
impl ServiceFilter {
    /// Returns `true` if this filter matches the given service and operation names.
    fn is_match(&self, service: Service, operation: Operation) -> bool {
        match self {
            ServiceFilter::All => true,
            ServiceFilter::Service(s) => s.eq_ignore_ascii_case(service),
            ServiceFilter::Operation(s, o) => {
                s.eq_ignore_ascii_case(service) && o.eq_ignore_ascii_case(operation)
            }
        }
    }
}

/// Structured attribute extraction logic for a specific AWS service.
///
/// Implement this trait to add custom OTel attributes to SDK call spans. Each
/// method corresponds to one phase of the interceptor pipeline and is called
/// with the service name, operation name, the phase-specific SDK context object,
/// and a mutable reference to the active span.
///
/// All methods have empty default implementations, so you only need to override
/// the phases you care about.
///
/// Register your implementation with
/// [`DefaultExtractor::register_attribute_extractor`]. It will be called after
/// the built-in per-service extractors (DynamoDB, S3, SQS) and before any
/// closure hooks.
///
/// # Examples
///
/// Adding a custom attribute to every DynamoDB `GetItem` span:
///
/// ```no_run
/// use awssdk_instrumentation::interceptor::{
///     AttributeExtractor, DefaultInterceptor, Operation, Service,
/// };
/// use awssdk_instrumentation::span_write::SpanWrite;
/// use aws_smithy_runtime_api::client::interceptors::context;
///
/// struct OrdersExtractor;
///
/// impl<SW: SpanWrite> AttributeExtractor<SW> for OrdersExtractor {
///     fn extract_input(
///         &self,
///         service: Service,
///         operation: Operation,
///         _input: &context::Input,
///         span: &mut SW,
///     ) {
///         if service == "DynamoDB" && operation == "GetItem" {
///             span.set_attribute("app.table", "orders");
///         }
///     }
/// }
///
/// let mut interceptor = DefaultInterceptor::new();
/// interceptor.extractor.register_attribute_extractor(OrdersExtractor);
/// ```
pub trait AttributeExtractor<SW: SpanWrite> {
    /// Extract attributes from the SDK input before serialization.
    ///
    /// Called once per SDK call, before the request is serialized. Use this
    /// phase to read typed input fields such as table names, bucket names, or
    /// queue URLs. Downcast `input` to the concrete SDK input type with
    /// `input.downcast_ref::<MyOperationInput>()`.
    fn extract_input(
        &self,
        _service: Service,
        _operation: Operation,
        _input: &context::Input,
        _span: &mut SW,
    ) {
    }

    /// Extract attributes from the serialized HTTP request.
    ///
    /// Called once per SDK call, after the input has been serialized into an
    /// HTTP request but before it is transmitted. Use this phase to read HTTP
    /// headers or the request URL.
    fn extract_request(
        &self,
        _service: Service,
        _operation: Operation,
        _request: &http::Request,
        _span: &mut SW,
    ) {
    }

    /// Extract attributes from the HTTP response before deserialization.
    ///
    /// Called once per SDK call, after the response is received but before it
    /// is deserialized. The built-in pipeline already sets
    /// `http.response.status_code`, `aws.request_id`, and
    /// `aws.extended_request_id` in this phase; use this method to add further
    /// response-header attributes.
    fn extract_response(
        &self,
        _service: Service,
        _operation: Operation,
        _response: &http::Response,
        _span: &mut SW,
    ) {
    }

    /// Extract attributes from the deserialized SDK output after execution.
    ///
    /// Called once per SDK call when the operation succeeds. Use this phase to
    /// read typed output fields such as consumed capacity, item counts, or
    /// message IDs. Downcast `output` to the concrete SDK output type with
    /// `output.downcast_ref::<MyOperationOutput>()`.
    fn extract_output(
        &self,
        _service: Service,
        _operation: Operation,
        _output: &context::Output,
        _span: &mut SW,
    ) {
    }

    /// Extract attributes from an operation error after execution.
    ///
    /// Called once per SDK call when the operation fails with a modeled service
    /// error. The built-in pipeline has already set `error.type` and the span
    /// status before this method is called. Override this method to refine those
    /// attributes — for example, by downcasting to the concrete per-operation
    /// error enum and reading `ProvideErrorMetadata`.
    fn extract_error(
        &self,
        _service: Service,
        _operation: Operation,
        _error: &context::Error,
        _span: &mut SW,
    ) {
    }
}

/// Boxed closure type for input-phase extraction hooks.
type InputHook<SW> =
    Box<dyn for<'a> Fn(Service<'a>, Operation<'a>, &'a context::Input, &'a mut SW) + Send + Sync>;
/// Boxed closure type for request-phase extraction hooks.
type RequestHook<SW> =
    Box<dyn for<'a> Fn(Service<'a>, Operation<'a>, &'a http::Request, &'a mut SW) + Send + Sync>;
/// Boxed closure type for response-phase extraction hooks.
type ResponseHook<SW> =
    Box<dyn for<'a> Fn(Service<'a>, Operation<'a>, &'a http::Response, &'a mut SW) + Send + Sync>;
/// Boxed closure type for output-phase extraction hooks.
type OutputHook<SW> =
    Box<dyn for<'a> Fn(Service<'a>, Operation<'a>, &'a context::Output, &'a mut SW) + Send + Sync>;
/// Boxed closure type for error-phase extraction hooks.
type ErrorHook<SW> =
    Box<dyn for<'a> Fn(Service<'a>, Operation<'a>, &'a context::Error, &'a mut SW) + Send + Sync>;

/// Built-in attribute extractor with per-service dispatch and user extension points.
///
/// `DefaultExtractor` is the core of the attribute extraction pipeline. For
/// each SDK call it:
///
/// 1. Dispatches to the appropriate built-in service extractor
///    ([`DynamoDBExtractor`], [`S3Extractor`], [`SQSExtractor`]) when the
///    corresponding feature is enabled.
/// 2. Calls every [`AttributeExtractor`] registered via
///    [`register_attribute_extractor`].
/// 3. Calls every closure hook registered via the `register_*_hook` methods,
///    filtered by the associated [`ServiceFilter`].
///
/// You do not construct `DefaultExtractor` directly. Access it through the
/// `extractor` field of [`TracingInterceptor`] or [`OtelInterceptor`] (both
/// exposed as [`DefaultInterceptor`]).
///
/// # Examples
///
/// Registering a closure hook that fires for every DynamoDB call:
///
/// ```no_run
/// use awssdk_instrumentation::{
///     interceptor::{DefaultInterceptor, ServiceFilter},
///     span_write::SpanWrite,
/// };
///
/// let mut interceptor = DefaultInterceptor::new();
/// interceptor.extractor.register_input_hook(
///     ServiceFilter::Service("DynamoDB"),
///     |_service, operation, _input, span| {
///         span.set_attribute("app.dynamo.operation", operation.to_owned());
///     },
/// );
/// ```
///
/// [`DynamoDBExtractor`]: crate::interceptor::extract::dynamodb::DynamoDBExtractor
/// [`S3Extractor`]: crate::interceptor::extract::s3::S3Extractor
/// [`SQSExtractor`]: crate::interceptor::extract::sqs::SQSExtractor
/// [`register_attribute_extractor`]: DefaultExtractor::register_attribute_extractor
/// [`TracingInterceptor`]: crate::interceptor::tracing::TracingInterceptor
/// [`OtelInterceptor`]: crate::interceptor::otel::OtelInterceptor
pub struct DefaultExtractor<SW: SpanWrite> {
    // Default extractors
    #[cfg(feature = "extract-dynamodb")]
    dynamodb_extractor: extract::dynamodb::DynamoDBExtractor,
    #[cfg(feature = "extract-s3")]
    s3_extractor: extract::s3::S3Extractor,
    #[cfg(feature = "extract-sqs")]
    sqs_extractor: extract::sqs::SQSExtractor,
    // User-registered trait-based extractors, run after built-in.
    custom_extractors: Vec<Box<dyn AttributeExtractor<SW> + Send + Sync>>,
    // User-registered closures, each scoped by a ServiceFilter, run last.
    input_hooks: Vec<(ServiceFilter, InputHook<SW>)>,
    request_hooks: Vec<(ServiceFilter, RequestHook<SW>)>,
    response_hooks: Vec<(ServiceFilter, ResponseHook<SW>)>,
    output_hooks: Vec<(ServiceFilter, OutputHook<SW>)>,
    error_hooks: Vec<(ServiceFilter, ErrorHook<SW>)>,
}
/// Non-exhaustive debug output for [`DefaultExtractor`] (omits closure fields).
impl<SW: SpanWrite> core::fmt::Debug for DefaultExtractor<SW> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultExtractor").finish_non_exhaustive()
    }
}

impl<SW: SpanWrite> DefaultExtractor<SW> {
    /// Creates a new `DefaultExtractor` with all built-in service extractors and no user extensions.
    fn new() -> Self {
        Self {
            #[cfg(feature = "extract-dynamodb")]
            dynamodb_extractor: extract::dynamodb::DynamoDBExtractor::new(),
            #[cfg(feature = "extract-s3")]
            s3_extractor: extract::s3::S3Extractor::new(),
            #[cfg(feature = "extract-sqs")]
            sqs_extractor: extract::sqs::SQSExtractor::new(),
            custom_extractors: Vec::new(),
            input_hooks: Vec::new(),
            request_hooks: Vec::new(),
            response_hooks: Vec::new(),
            output_hooks: Vec::new(),
            error_hooks: Vec::new(),
        }
    }

    /// Register a closure that runs during the input phase for matching SDK calls.
    ///
    /// The hook receives the service name, operation name, the type-erased SDK input,
    /// and a mutable reference to the active span. It is called after the
    /// built-in service extractors and before any previously registered hooks.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use awssdk_instrumentation::{
    ///     interceptor::{DefaultInterceptor, ServiceFilter},
    ///     span_write::SpanWrite,
    /// };
    ///
    /// let mut interceptor = DefaultInterceptor::new();
    /// interceptor.extractor.register_input_hook(
    ///     ServiceFilter::Operation("DynamoDB", "GetItem"),
    ///     |_service, _operation, _input, span| {
    ///         span.set_attribute("app.table", "orders");
    ///     },
    /// );
    /// ```
    pub fn register_input_hook<H>(&mut self, filter: ServiceFilter, hook: H)
    where
        H: for<'a> Fn(Service<'a>, Operation<'a>, &'a context::Input, &'a mut SW),
        H: Send + Sync + 'static,
    {
        self.input_hooks.push((filter, Box::new(hook)));
    }

    /// Register a closure that runs during the request phase for matching SDK calls.
    ///
    /// The hook receives the service name, operation name, the serialized HTTP
    /// request, and a mutable reference to the active span. Use this phase to
    /// read HTTP headers or the request URL.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use awssdk_instrumentation::{
    ///     interceptor::{DefaultInterceptor, ServiceFilter},
    ///     span_write::SpanWrite,
    /// };
    ///
    /// let mut interceptor = DefaultInterceptor::new();
    /// interceptor.extractor.register_request_hook(
    ///     ServiceFilter::All,
    ///     |_service, _operation, request, span| {
    ///         if let Some(host) = request.headers().get("host") {
    ///             span.set_attribute("http.request.host", host.to_owned());
    ///         }
    ///     },
    /// );
    /// ```
    pub fn register_request_hook<H>(&mut self, filter: ServiceFilter, hook: H)
    where
        H: for<'a> Fn(Service<'a>, Operation<'a>, &'a http::Request, &'a mut SW),
        H: Send + Sync + 'static,
    {
        self.request_hooks.push((filter, Box::new(hook)));
    }

    /// Register a closure that runs during the response phase for matching SDK calls.
    ///
    /// The hook receives the service name, operation name, the raw HTTP response,
    /// and a mutable reference to the active span. The built-in pipeline has
    /// already set `http.response.status_code`, `aws.request_id`, and
    /// `aws.extended_request_id` before this hook is called.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use awssdk_instrumentation::{
    ///     interceptor::{DefaultInterceptor, ServiceFilter},
    ///     span_write::SpanWrite,
    /// };
    ///
    /// let mut interceptor = DefaultInterceptor::new();
    /// interceptor.extractor.register_response_hook(
    ///     ServiceFilter::Service("DynamoDB"),
    ///     |_service, _operation, response, span| {
    ///         if let Some(crc) = response.headers().get("x-amz-crc32") {
    ///             span.set_attribute("aws.dynamodb.crc32", crc.to_owned());
    ///         }
    ///     },
    /// );
    /// ```
    pub fn register_response_hook<H>(&mut self, filter: ServiceFilter, hook: H)
    where
        H: for<'a> Fn(Service<'a>, Operation<'a>, &'a http::Response, &'a mut SW),
        H: Send + Sync + 'static,
    {
        self.response_hooks.push((filter, Box::new(hook)));
    }

    /// Register a closure that runs during the output phase for matching SDK calls.
    ///
    /// The hook receives the service name, operation name, the type-erased SDK
    /// output, and a mutable reference to the active span. Called only when the
    /// operation succeeds.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use awssdk_instrumentation::{
    ///     interceptor::{DefaultInterceptor, ServiceFilter},
    ///     span_write::SpanWrite,
    /// };
    ///
    /// let mut interceptor = DefaultInterceptor::new();
    /// interceptor.extractor.register_output_hook(
    ///     ServiceFilter::Operation("DynamoDB", "GetItem"),
    ///     |_service, _operation, output, span| {
    ///         span.set_attribute("app.item.found", true);
    ///     },
    /// );
    /// ```
    pub fn register_output_hook<H>(&mut self, filter: ServiceFilter, hook: H)
    where
        H: for<'a> Fn(Service<'a>, Operation<'a>, &'a context::Output, &'a mut SW),
        H: Send + Sync + 'static,
    {
        self.output_hooks.push((filter, Box::new(hook)));
    }

    /// Register a closure that runs during the error phase for matching SDK calls.
    ///
    /// The hook receives the service name, operation name, the type-erased
    /// operation error, and a mutable reference to the active span. The
    /// built-in pipeline has already set `error.type` and the span status
    /// before this hook is called; use this hook to augment or override those
    /// attributes.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use awssdk_instrumentation::{
    ///     interceptor::{DefaultInterceptor, ServiceFilter},
    ///     span_write::SpanWrite,
    /// };
    ///
    /// let mut interceptor = DefaultInterceptor::new();
    /// interceptor.extractor.register_error_hook(
    ///     ServiceFilter::Service("DynamoDB"),
    ///     |_service, _operation, _error, span| {
    ///         span.set_attribute("app.dynamo.error", true);
    ///     },
    /// );
    /// ```
    pub fn register_error_hook<H>(&mut self, filter: ServiceFilter, hook: H)
    where
        H: for<'a> Fn(Service<'a>, Operation<'a>, &'a context::Error, &'a mut SW),
        H: Send + Sync + 'static,
    {
        self.error_hooks.push((filter, Box::new(hook)));
    }

    /// Register a trait-based extractor for structured attribute extraction logic.
    ///
    /// The extractor is called after the built-in per-service extractors and
    /// before any closure hooks registered with the `register_*_hook` methods.
    /// Multiple extractors can be registered; they are called in registration
    /// order.
    ///
    /// Prefer this method over closure hooks when the extraction logic is
    /// complex enough to warrant a dedicated type.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use awssdk_instrumentation::{
    ///     interceptor::{
    ///         AttributeExtractor, DefaultInterceptor, Operation, Service,
    ///     },
    ///     span_write::SpanWrite,
    /// };
    /// use aws_smithy_runtime_api::client::interceptors::context;
    ///
    /// struct OrdersExtractor;
    ///
    /// impl<SW: SpanWrite> AttributeExtractor<SW> for OrdersExtractor {
    ///     fn extract_input(
    ///         &self,
    ///         _service: Service,
    ///         _operation: Operation,
    ///         _input: &context::Input,
    ///         span: &mut SW,
    ///     ) {
    ///         span.set_attribute("app.table", "orders");
    ///     }
    /// }
    ///
    /// let mut interceptor = DefaultInterceptor::new();
    /// interceptor.extractor.register_attribute_extractor(OrdersExtractor);
    /// ```
    pub fn register_attribute_extractor<AE>(&mut self, extractor: AE)
    where
        AE: AttributeExtractor<SW>,
        AE: Send + Sync + 'static,
    {
        self.custom_extractors.push(Box::new(extractor));
    }
}

/// Dispatches an extraction phase to built-in service extractors, custom extractors, and closure hooks.
macro_rules! call_extractors {
    ($self:ident $service:ident $operation:ident $method:ident $hooks:ident $parameter:ident $span:ident) => {
        // Internal extractors
        match $service {
            #[cfg(feature = "extract-dynamodb")]
            "DynamoDB" => $self
                .dynamodb_extractor
                .$method($service, $operation, $parameter, $span),
            #[cfg(feature = "extract-s3")]
            "S3" => $self
                .s3_extractor
                .$method($service, $operation, $parameter, $span),
            #[cfg(feature = "extract-sqs")]
            "SQS" => $self
                .sqs_extractor
                .$method($service, $operation, $parameter, $span),
            _ => {}
        }

        // User defined extractors if any
        for custom_extractors in $self.custom_extractors.iter() {
            custom_extractors.$method($service, $operation, $parameter, $span);
        }

        // User defined hooks if any
        for hook in $self
            .$hooks
            .iter()
            .filter_map(|(filter, hook)| filter.is_match($service, $operation).then_some(hook))
        {
            hook.as_ref()($service, $operation, $parameter, $span);
        }
    };
}

impl<SW: SpanWrite> DefaultExtractor<SW> {
    /// Runs the input extraction phase: sets the cloud region, parses the service/operation from
    /// the tracing span name, and dispatches to all registered extractors and hooks.
    fn read_before_execution(
        &self,
        context: &context::BeforeSerializationInterceptorContextRef<'_>,
        cfg: &mut ConfigBag,
        span: &mut SW,
    ) -> Result<(), BoxError> {
        log::trace!("CFG: {:?}", cfg);

        span.set_attribute(
            semco::CLOUD_REGION,
            cfg.load::<Region>()
                .ok_or("No Region in the ConfigBag")?
                .to_string(),
        );

        let sdk_operation = {
            #[cfg(feature = "tracing-backend")]
            let span = {
                use ::tracing::Span;
                use utils::StorableOption;

                // In the tracing context, the Span we want will have been put in the ConfigBag!
                let so_span = cfg.load::<StorableOption<Span>>().ok_or(
                    "AWS SDK operation top-level tracing:Span not found, \
                        it likely means AWS changed their API, \
                        please contact the maintainer immediately.",
                )?;
                so_span.as_ref().expect("StorableOption always set to Some")
            };

            #[cfg(all(feature = "otel-backend", not(feature = "tracing-backend")))]
            let (_guard, span) = {
                use utils::SpanPauser;

                SpanPauser::pause_until(|span| {
                    span.metadata()
                        .map(|metadata| metadata.target().contains("::operation::"))
                        .unwrap_or_default()
                })
                .ok_or(
                    "AWS SDK operation top-level tracing:Span not found, \
                    it likely means AWS changed their API, \
                    please contact the maintainer immediately.",
                )?
            };

            let span_name = span
                .metadata()
                .ok_or("tracing::Span metadata not enabled")?
                .name();
            let (service, operation) = span_name.split_once('.').ok_or_else(|| {
                format!(
                    "AWS SDK operation top-level tracing:Span name does not have \
                    the expected form: {span_name}, it likely means AWS changed \
                    their API, please contact the maintainer immediately."
                )
            })?;
            AwsSdkOperation::new(service, operation)
        };

        let service = sdk_operation.service();
        let operation = sdk_operation.operation();

        let input = context.input();

        log::trace!("INPUT: {:?}", input);

        call_extractors!(self service operation extract_input input_hooks input span);

        cfg.interceptor_state().store_put(sdk_operation);

        Ok(())
    }

    /// Runs the request extraction phase: dispatches to all registered extractors and hooks with the serialized HTTP request.
    fn read_after_serialization(
        &self,
        context: &context::BeforeTransmitInterceptorContextRef<'_>,
        cfg: &mut ConfigBag,
        span: &mut SW,
    ) -> Result<(), BoxError> {
        log::trace!("CFG: {:?}", cfg);

        let (service, operation) = extract_service_operation(cfg);

        let request = context.request();

        log::trace!("REQUEST: {:?}", request);

        call_extractors!(self service operation extract_request request_hooks request span);

        Ok(())
    }

    /// Runs the response extraction phase: sets HTTP status, request ID, extended request ID,
    /// and dispatches to all registered extractors and hooks.
    fn read_before_deserialization(
        &self,
        context: &context::BeforeDeserializationInterceptorContextRef<'_>,
        cfg: &mut ConfigBag,
        span: &mut SW,
    ) -> Result<(), BoxError> {
        log::trace!("CFG: {:?}", cfg);

        let (service, operation) = extract_service_operation(cfg);

        let response = context.response();

        log::trace!("RESPONSE: {:?}", response);

        span.set_attribute(
            semco::HTTP_RESPONSE_STATUS_CODE,
            response.status().as_u16() as i64,
        );

        if let Some(req_id) = RequestId::request_id(response) {
            log::trace!("REQ_ID: {req_id}");
            span.set_attribute(semco::AWS_REQUEST_ID, req_id.to_owned());
        }

        if let Some(extended_id) = response.headers().get("x-amz-id-2") {
            log::trace!("EXTENDED_REQ_ID: {extended_id}");
            span.set_attribute(semco::AWS_EXTENDED_REQUEST_ID, extended_id.to_owned());
        }

        call_extractors!(self service operation extract_response response_hooks response span);

        Ok(())
    }
    /// Runs the output/error extraction phase: dispatches to extractors on success, or sets
    /// `error.type` and span status on failure.
    fn read_after_execution(
        &self,
        context: &context::FinalizerInterceptorContextRef<'_>,
        cfg: &mut ConfigBag,
        span: &mut SW,
    ) -> Result<(), BoxError> {
        log::trace!("CFG: {:?}", cfg);

        let (service, operation) = extract_service_operation(cfg);

        let output_or_error = context.output_or_error();

        log::trace!("OUTPUT_OR_ERROR: {:?}", output_or_error);

        match output_or_error {
            Some(Ok(output)) => {
                call_extractors!(self service operation extract_output output_hooks output span);
            }
            Some(Err(orchestration_error)) => {
                if let Some(op_error) = orchestration_error.as_operation_error() {
                    // Operation error — the service returned a modeled error.
                    //
                    // We cannot access `ProvideErrorMetadata` here because the
                    // SDK's type-erasure (`TypeErasedError`) only preserves the
                    // `std::error::Error` vtable, not `ProvideErrorMetadata`.
                    // Downcasting would require knowing the concrete per-operation
                    // error enum at compile time (that's what `extract_error` on
                    // service extractors is for).
                    //
                    // Instead we parse the Display output. Every codegen'd SDK
                    // operation error produces "ErrorCode: human message" or just
                    // "ErrorCode" (no message). This format is emitted by the
                    // `Display` impl generated in each operation module (e.g.
                    // `aws-sdk-dynamodb/src/operation/put_item.rs`). The inner
                    // variant types follow the same pattern — see for example
                    // `aws-sdk-dynamodb/src/types/error/_conditional_check_failed_exception.rs`.
                    //
                    // If this parsing ever breaks, check the generated Display
                    // impl in the SDK crate for the service in question — look
                    // for `impl ::std::fmt::Display for <Operation>Error` in
                    // `src/operation/<snake_op>/builders.rs` (or the parent
                    // `src/operation/<snake_op>.rs` depending on SDK version).
                    let display = format!("{op_error}");
                    let (error_type, message) = match display.split_once(": ") {
                        Some((code, msg)) => (code, msg),
                        None => (display.as_str(), display.as_str()),
                    };
                    log::debug!("operation error: {display}");

                    span.set_attribute(semco::ERROR_TYPE, error_type.to_owned());
                    span.set_status(Status::error(message.to_owned()));

                    // Let service extractors and user hooks refine error attributes.
                    let error = op_error;
                    call_extractors!(self service operation extract_error error_hooks error span);
                } else if let Some(connector_error) = orchestration_error.as_connector_error() {
                    // Connector error — network or dispatch failure.
                    let message = format!("{connector_error}");
                    log::debug!("connector error: {message}");

                    span.set_attribute(semco::ERROR_TYPE, "CONNECTOR".to_owned());
                    span.set_status(Status::error(message));
                } else if orchestration_error.is_timeout_error() {
                    let message = format!("{orchestration_error}");
                    log::debug!("timeout error: {message}");

                    span.set_attribute(semco::ERROR_TYPE, "TIMEOUT".to_owned());
                    span.set_status(Status::error(message));
                } else {
                    // Interceptor, response, or other errors.
                    let message = format!("{orchestration_error}");
                    log::debug!("orchestration error: {message}");

                    span.set_attribute(semco::ERROR_TYPE, "_OTHER".to_owned());
                    span.set_status(Status::error(message));
                }
            }
            None => {
                // No output or error — the SDK failed before producing either.
                log::debug!("no output or error received");
                span.set_attribute(semco::ERROR_TYPE, "_OTHER".to_owned());
                span.set_status(Status::error(
                    "SDK completed without output or error".to_owned(),
                ));
            }
        }

        Ok(())
    }
}
