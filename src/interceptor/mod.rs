// Interceptor module — AttributeExtractor trait, DefaultExtractor, ServiceFilter,
// closure registration, and service dispatch logic.

// Semantic convention constants not yet available in the `opentelemetry-semantic-conventions`
// crate. Named identically to the upstream pattern so the compiler will error once the
// crate exports them and we can remove these definitions.
pub const DB_SYSTEM_NAME: &str = "db.system.name";
pub const RPC_SYSTEM_NAME: &str = "rpc.system.name";
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

#[cfg(feature = "tracing-backend")]
pub type DefaultInterceptor = tracing::TracingInterceptor;

#[cfg(all(feature = "otel-backend", not(feature = "tracing-backend")))]
pub type DefaultInterceptor = otel::OtelInterceptor;

use aws_smithy_runtime_api::{box_error::BoxError, client::interceptors::context, http};
use aws_smithy_types::config_bag::ConfigBag;
use aws_types::{region::Region, request_id::RequestId};

use opentelemetry::{Value, trace::Status};
use opentelemetry_semantic_conventions::attribute as semco;

use utils::{AwsSdkOperation, SpanPauser, extract_service_operation};

// Backend-agnostic interface for injecting attributes and status into a span.
pub trait SpanWrite {
    fn set_attribute(&mut self, key: &'static str, value: impl Into<Value>);
    fn set_status(&mut self, code: Status);
}

pub type Service<'a> = &'a str;
pub type Operation<'a> = &'a str;

// Scopes when a registered closure or extractor fires.
pub enum ServiceFilter {
    // Matches all services and operations.
    All,
    // Matches a specific service (e.g. "DynamoDB").
    Service(Service<'static>),
    // Matches a specific service + operation (e.g. "DynamoDB", "PutItem").
    Operation(Service<'static>, Operation<'static>),
}
impl ServiceFilter {
    fn is_match(&self, service: Service, operation: Operation) -> bool {
        match self {
            ServiceFilter::All => true,
            ServiceFilter::Service(s) if *s == service => true,
            ServiceFilter::Operation(s, o) if *s == service && *o == operation => true,
            _ => false,
        }
    }
}

// Trait for structured attribute extraction logic, generic over the SpanWrite backend.
pub trait AttributeExtractor<SW: SpanWrite> {
    // Extract attributes from the Input before execution.
    fn extract_input(
        &self,
        _service: Service,
        _operation: Operation,
        _input: &context::Input,
        _span: &mut SW,
    ) {
    }
    // Extract attributes from the Request after serialization.
    fn extract_request(
        &self,
        _service: Service,
        _operation: Operation,
        _request: &http::Request,
        _span: &mut SW,
    ) {
    }
    // Extract attributes from the Response before deserialization.
    fn extract_response(
        &self,
        _service: Service,
        _operation: Operation,
        _response: &http::Response,
        _span: &mut SW,
    ) {
    }
    // Extract attributes from the Output after execution.
    fn extract_output(
        &self,
        _service: Service,
        _operation: Operation,
        _output: &context::Output,
        _span: &mut SW,
    ) {
    }
}

// Type alias for registered input extraction closures.
type InputHook<SW> =
    Box<dyn for<'a> Fn(Service<'a>, Operation<'a>, &'a context::Input, &'a mut SW) + Send + Sync>;
type RequestHook<SW> =
    Box<dyn for<'a> Fn(Service<'a>, Operation<'a>, &'a http::Request, &'a mut SW) + Send + Sync>;
type ResponseHook<SW> =
    Box<dyn for<'a> Fn(Service<'a>, Operation<'a>, &'a http::Response, &'a mut SW) + Send + Sync>;
type OutputHook<SW> =
    Box<dyn for<'a> Fn(Service<'a>, Operation<'a>, &'a context::Output, &'a mut SW) + Send + Sync>;

// The built-in extractor that dispatches by service/operation and supports user extensions.
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
}
impl<SW: SpanWrite> core::fmt::Debug for DefaultExtractor<SW> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultExtractor").finish_non_exhaustive()
    }
}

impl<SW: SpanWrite> DefaultExtractor<SW> {
    // Create a new DefaultExtractor with no user extensions.
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
        }
    }

    // Register a closure for input extraction, scoped by a ServiceFilter.
    pub fn register_input_hook<H>(&mut self, filter: ServiceFilter, hook: H)
    where
        H: for<'a> Fn(Service<'a>, Operation<'a>, &'a context::Input, &'a mut SW),
        H: Send + Sync + 'static,
    {
        self.input_hooks.push((filter, Box::new(hook)));
    }

    // Register a closure for output extraction, scoped by a ServiceFilter.
    pub fn register_request_hook<H>(&mut self, filter: ServiceFilter, hook: H)
    where
        H: for<'a> Fn(Service<'a>, Operation<'a>, &'a http::Request, &'a mut SW),
        H: Send + Sync + 'static,
    {
        self.request_hooks.push((filter, Box::new(hook)));
    }

    // Register a closure for input extraction, scoped by a ServiceFilter.
    pub fn register_response_hook<H>(&mut self, filter: ServiceFilter, hook: H)
    where
        H: for<'a> Fn(Service<'a>, Operation<'a>, &'a http::Response, &'a mut SW),
        H: Send + Sync + 'static,
    {
        self.response_hooks.push((filter, Box::new(hook)));
    }

    // Register a closure for output extraction, scoped by a ServiceFilter.
    pub fn register_output_hook<H>(&mut self, filter: ServiceFilter, hook: H)
    where
        H: for<'a> Fn(Service<'a>, Operation<'a>, &'a context::Output, &'a mut SW),
        H: Send + Sync + 'static,
    {
        self.output_hooks.push((filter, Box::new(hook)));
    }

    // Register a trait-based extractor for structured extraction logic.
    pub fn register_attribute_extractor<AE>(&mut self, extractor: AE)
    where
        AE: AttributeExtractor<SW>,
        AE: Send + Sync + 'static,
    {
        self.custom_extractors.push(Box::new(extractor));
    }
}

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
                .expect("region MUST be configured on requests")
                .to_string(),
        );
        let sdk_operation = if let Some((_guard, span)) = SpanPauser::pause_until(|span| {
            span.metadata()
                .map(|metadata| metadata.target().contains("::operation::"))
                .unwrap_or_default()
        }) {
            let span_name = span
                .metadata()
                .ok_or("tracing::Span metadata not enabled")?
                .name();
            let (service, operation) = span_name.split_once('.').ok_or_else(|| {
                format!("AWS SDK operation top-level tracing:Span name does not have the expected form: {span_name}, it likely means AWS changed their API, please contact the maintainer immediatly.")
            })?;
            AwsSdkOperation::new(service, operation)
        } else {
            return Err(
                "AWS SDK operation top-level tracing:Span not found, it likely means AWS changed their API, please contact the maintainer immediatly.",
            )?;
        };

        let service = sdk_operation.service();
        let operation = sdk_operation.operation();

        let input = context.input();

        log::trace!("INPUT: {:?}", input);

        call_extractors!(self service operation extract_input input_hooks input span);

        cfg.interceptor_state().store_put(sdk_operation);

        Ok(())
    }

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
    fn read_after_execution(
        &self,
        context: &context::FinalizerInterceptorContextRef<'_>,
        cfg: &mut ConfigBag,
        span: &mut SW,
    ) -> Result<(), BoxError> {
        log::trace!("CFG: {:?}", cfg);

        let (service, operation) = extract_service_operation(cfg);

        let ouput_or_error = context.output_or_error();

        log::trace!("OUTPUT_OR_ERROR: {:?}", ouput_or_error);

        match ouput_or_error {
            Some(Ok(output)) => {
                call_extractors!(self service operation extract_output output_hooks output span);
            }
            Some(Err(orchestration_error)) => {
                if let Some(op_error) = orchestration_error.as_operation_error() {
                    log::debug!("{op_error:?}");
                } else if let Some(con_error) = orchestration_error.as_connector_error() {
                    log::debug!("{con_error:?}");
                } else {
                    log::debug!("{orchestration_error:?}");
                }
            }
            None => {
                log::debug!("No output received");
            }
        }

        Ok(())
    }
}
