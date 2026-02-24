use opentelemetry::{SpanId, TraceFlags, TraceId};

pub(super) struct XRayTraceHeader {
    pub(super) trace_id: TraceId,
    pub(super) parent_id: SpanId,
    pub(super) sampled: TraceFlags,
}
impl XRayTraceHeader {
    const ROOT: &str = "Root";
    const PARENT: &str = "Parent";
    const SAMPLE: &str = "Sampled";
    const HEADER_DELIMITER: &str = ";";
}
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
                _ => return Err("Invalid Trace header".to_owned()),
            }
        }

        if !(trace_id_collected && parent_id_collected && sampled_collected) {
            return Err("Invalid Trace header".to_owned());
        }

        Ok(xray_header)
    }
}

/// Represent the possible values for the OpenTelemetry `faas.trigger` attribute.
/// See <https://opentelemetry.io/docs/specs/semconv/attributes-registry/faas/> for more details.
#[derive(Default, Clone, Copy)]
#[non_exhaustive]
pub enum OTelFaasTrigger {
    /// A response to some data source operation such as a database or filesystem read/write
    #[default]
    Datasource,
    /// To provide an answer to an inbound HTTP request
    Http,
    /// A function is set to be executed when messages are sent to a messaging system
    PubSub,
    /// A function is scheduled to be executed regularly
    Timer,
    /// If none of the others apply
    Other,
}

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
