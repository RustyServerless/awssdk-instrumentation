//! Utilities for parsing the X-Ray trace header and defining the `faas.trigger` attribute.

use opentelemetry::{SpanId, TraceFlags, TraceId};

/// Parsed X-Ray trace header containing the trace ID, parent span ID, and sampling flag.
#[derive(Debug)]
pub(super) struct XRayTraceHeader {
    pub(super) trace_id: TraceId,
    pub(super) parent_id: SpanId,
    pub(super) sampled: TraceFlags,
}
impl XRayTraceHeader {
    /// `Root` field key in the X-Ray trace header.
    const ROOT: &str = "Root";
    /// `Parent` field key in the X-Ray trace header.
    const PARENT: &str = "Parent";
    /// `Sampled` field key in the X-Ray trace header.
    const SAMPLE: &str = "Sampled";
    /// `Lineage` field key in the X-Ray trace header (ignored during parsing).
    const LINEAGE: &str = "Lineage";
    /// Delimiter between key-value pairs in the X-Ray trace header.
    const HEADER_DELIMITER: &str = ";";
}
/// Parses an X-Ray trace header string (e.g. from `_X_AMZN_TRACE_ID`) into an [`XRayTraceHeader`].
impl core::str::FromStr for XRayTraceHeader {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut xray_header = Self {
            trace_id: TraceId::INVALID,
            parent_id: SpanId::INVALID,
            sampled: TraceFlags::SAMPLED,
        };
        let mut trace_id_collected = false;
        let mut parent_id_collected = false;
        let mut sampled_collected = false;

        fn map_err(e: impl ToString) -> String {
            e.to_string()
        }
        for (key, value) in s
            .split(Self::HEADER_DELIMITER)
            .filter_map(|part| part.split_once('='))
        {
            match key {
                Self::ROOT if !trace_id_collected => {
                    xray_header.trace_id =
                        TraceId::from_hex(&value.split('-').skip(1).collect::<String>())
                            .map_err(map_err)?;
                    trace_id_collected = true;
                }
                Self::PARENT if !parent_id_collected => {
                    xray_header.parent_id = SpanId::from_hex(value).map_err(map_err)?;
                    parent_id_collected = true;
                }
                Self::SAMPLE if !sampled_collected => {
                    xray_header.sampled = match value {
                        "0" => TraceFlags::NOT_SAMPLED,
                        "1" => TraceFlags::SAMPLED,
                        _ => return Err("Invalid Trace header".to_owned()),
                    };
                    sampled_collected = true;
                }
                Self::LINEAGE => {
                    // Ignored
                }
                // Ignore unrecognized keys — the X-Ray header format may be extended
                // with new fields in the future
                _ => {}
            }
        }

        if !(trace_id_collected && parent_id_collected && sampled_collected) {
            return Err("Invalid Trace header".to_owned());
        }

        Ok(xray_header)
    }
}

/// The value of the OpenTelemetry `faas.trigger` attribute for a Lambda invocation.
///
/// Pass a variant to [`TracingLayer::with_trigger`] to describe what kind of
/// event triggers your Lambda function. The value is set on the per-invocation
/// span as the `faas.trigger` attribute.
///
/// The default variant is [`Datasource`], which is appropriate for Lambda
/// functions that read from or write to a data store such as DynamoDB or S3.
///
/// See the [OTel FaaS attributes registry](https://opentelemetry.io/docs/specs/semconv/attributes-registry/faas/)
/// for the full specification.
///
/// # Examples
///
/// ```
/// use awssdk_instrumentation::lambda::OTelFaasTrigger;
///
/// assert_eq!(OTelFaasTrigger::Datasource.to_string(), "datasource");
/// assert_eq!(OTelFaasTrigger::Http.to_string(), "http");
/// assert_eq!(OTelFaasTrigger::PubSub.to_string(), "pubsub");
/// assert_eq!(OTelFaasTrigger::Timer.to_string(), "timer");
/// assert_eq!(OTelFaasTrigger::Other.to_string(), "other");
/// ```
///
/// [`TracingLayer::with_trigger`]: crate::lambda::layer::TracingLayer::with_trigger
/// [`Datasource`]: OTelFaasTrigger::Datasource
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub enum OTelFaasTrigger {
    /// A response to a data source operation such as a database or filesystem read/write.
    ///
    /// This is the default. Use it for Lambda functions triggered by DynamoDB
    /// Streams, S3 events, or other data-store events.
    #[default]
    Datasource,
    /// A response to an inbound HTTP request.
    ///
    /// Use this for Lambda functions fronted by API Gateway or a Function URL.
    Http,
    /// A function invoked when messages are sent to a messaging system.
    ///
    /// Use this for Lambda functions triggered by SQS, SNS, or EventBridge.
    PubSub,
    /// A function scheduled to run at regular intervals.
    ///
    /// Use this for Lambda functions triggered by EventBridge Scheduler or
    /// CloudWatch Events rules.
    Timer,
    /// None of the other trigger types apply.
    Other,
}

/// Formats the trigger as the lowercase string value used in the `faas.trigger`
/// OTel attribute (e.g. `"datasource"`, `"http"`).
impl std::fmt::Display for OTelFaasTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OTelFaasTrigger::Datasource => write!(f, "datasource"),
            OTelFaasTrigger::Http => write!(f, "http"),
            OTelFaasTrigger::PubSub => write!(f, "pubsub"),
            OTelFaasTrigger::Timer => write!(f, "timer"),
            OTelFaasTrigger::Other => write!(f, "other"),
        }
    }
}
