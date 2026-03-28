use std::ops::{Deref, DerefMut};

use aws_smithy_types::config_bag::{ConfigBag, Storable, StoreReplace};
use tracing::Span;

use super::{Operation, Service};

#[derive(Debug)]
pub struct StorableOption<T: core::fmt::Debug>(Option<T>);

impl<T: core::fmt::Debug + Send + Sync + 'static> Storable for StorableOption<T> {
    type Storer = StoreReplace<Self>;
}
impl<T: core::fmt::Debug> Default for StorableOption<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T: core::fmt::Debug> StorableOption<T> {
    pub fn new(content: T) -> Self {
        Self(Some(content))
    }
}

impl<T: core::fmt::Debug> Deref for StorableOption<T> {
    type Target = Option<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T: core::fmt::Debug> DerefMut for StorableOption<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
pub(super) struct AwsSdkOperation {
    service: String,
    operation: String,
}
impl AwsSdkOperation {
    pub fn new(service: impl Into<String>, operation: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            operation: operation.into(),
        }
    }
    pub fn service(&self) -> Service<'_> {
        &self.service
    }
    pub fn operation(&self) -> Operation<'_> {
        &self.operation
    }
}
impl Storable for AwsSdkOperation {
    type Storer = StoreReplace<Self>;
}

pub(super) struct PausedSpanGuard {
    paused_spans: Vec<Span>,
}
impl Drop for PausedSpanGuard {
    fn drop(&mut self) {
        // When droping, re-enable the spans in the reverse order of disablement
        while let Some(span) = self.paused_spans.pop() {
            if let Some(id) = span.id() {
                log::trace!("re-enabling span: {span:?}");
                tracing::dispatcher::get_default(|d| d.enter(&id));
            }
        }
    }
}

pub(super) struct SpanPauser;
impl SpanPauser {
    pub fn pause_until<F: Fn(&Span) -> bool>(predicate: F) -> Option<(PausedSpanGuard, Span)> {
        let mut guard = PausedSpanGuard {
            paused_spans: vec![],
        };

        loop {
            // Get the current span
            let span = Span::current();

            // If it is disabled, we consider we cannot go further up
            if span.is_disabled() {
                log::trace!("hit disabled span: {span:?}");
                break;
            }

            // If it matches the predicate, then return it as it is the one we are looking for
            if predicate(&span) {
                log::trace!("span match predicate: {span:?}");
                return Some((guard, span));
            }

            // Else disable the span, store it, and loop around to test the parent.
            log::trace!("disabling span temporarilly: {span:?}");
            tracing::dispatcher::get_default(|d| d.exit(&span.id().expect("enabled span has id")));
            guard.paused_spans.push(span);
        }

        // Re-enable the paused spans if any
        drop(guard);
        None
    }
}

pub fn extract_service_operation(cfg: &ConfigBag) -> (Service<'_>, Operation<'_>) {
    let aws_sdk_operation = cfg
        .load::<AwsSdkOperation>()
        .expect("metadata always present");
    (aws_sdk_operation.service(), aws_sdk_operation.operation())
}
