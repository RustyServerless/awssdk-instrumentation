//! Internal utilities for interceptor span management and ConfigBag helpers.

use std::ops::{Deref, DerefMut};

use aws_smithy_types::config_bag::{ConfigBag, Storable, StoreReplace};
use tracing::Span;

use super::{Operation, Service};

/// Newtype around `Option<T>` that implements [`Storable`] for use in a [`ConfigBag`].
#[derive(Debug)]
pub struct StorableOption<T: core::fmt::Debug>(Option<T>);

/// [`Storable`] impl allowing `StorableOption<T>` to be stored in a [`ConfigBag`].
impl<T: core::fmt::Debug + Send + Sync + 'static> Storable for StorableOption<T> {
    type Storer = StoreReplace<Self>;
}
/// Defaults to `None`.
impl<T: core::fmt::Debug> Default for StorableOption<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T: core::fmt::Debug> StorableOption<T> {
    /// Wraps `content` in `Some`, producing a non-empty [`StorableOption`].
    pub fn new(content: T) -> Self {
        Self(Some(content))
    }
}

/// Transparent access to the inner `Option<T>`.
impl<T: core::fmt::Debug> Deref for StorableOption<T> {
    type Target = Option<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
/// Transparent mutable access to the inner `Option<T>`.
impl<T: core::fmt::Debug> DerefMut for StorableOption<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Parsed AWS SDK service and operation names stored in the [`ConfigBag`].
#[derive(Debug)]
pub(super) struct AwsSdkOperation {
    service: String,
    operation: String,
}
impl AwsSdkOperation {
    /// Creates a new `AwsSdkOperation` from the given service and operation names.
    pub fn new(service: impl Into<String>, operation: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            operation: operation.into(),
        }
    }
    /// Returns the service name (e.g. `"DynamoDB"`).
    pub fn service(&self) -> Service<'_> {
        &self.service
    }
    /// Returns the operation name (e.g. `"GetItem"`).
    pub fn operation(&self) -> Operation<'_> {
        &self.operation
    }
}
/// [`Storable`] impl allowing [`AwsSdkOperation`] to be stored in a [`ConfigBag`].
impl Storable for AwsSdkOperation {
    type Storer = StoreReplace<Self>;
}

/// RAII guard that re-enables tracing spans that were temporarily paused by [`SpanPauser`].
pub(super) struct PausedSpanGuard {
    paused_spans: Vec<Span>,
}
/// Re-enables all paused spans in reverse order when the guard is dropped.
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

/// Walks up the tracing span stack, temporarily pausing spans until a predicate matches.
pub(super) struct SpanPauser;
impl SpanPauser {
    /// Pauses spans from the top of the stack until `predicate` returns `true`, returning the
    /// matching span and a guard that restores the paused spans on drop.
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

/// Extracts the [`AwsSdkOperation`] stored in the [`ConfigBag`] and returns its service and operation names.
pub fn extract_service_operation(cfg: &ConfigBag) -> (Service<'_>, Operation<'_>) {
    let aws_sdk_operation = cfg
        .load::<AwsSdkOperation>()
        .expect("metadata always present");
    (aws_sdk_operation.service(), aws_sdk_operation.operation())
}

#[cfg(test)]
mod tests {
    use tracing::info_span;

    use super::*;

    #[test]
    fn pause_until_finds_matching_ancestor_and_restores_on_drop() {
        tracing::subscriber::with_default(tracing_subscriber::registry(), || {
            let span_a = info_span!(target: "aws_sdk_x::operation::y", "A");
            let _enter_a = span_a.enter();
            let span_b = info_span!("B");
            let _enter_b = span_b.enter();
            let span_c = info_span!("C");
            let _enter_c = span_c.enter();

            let result =
                SpanPauser::pause_until(|s| s.metadata().map(|m| m.name() == "A").unwrap_or(false));

            let (guard, matched) = result.expect("predicate should match span A");
            assert_eq!(matched.id(), span_a.id());
            assert_eq!(Span::current().id(), span_a.id());

            drop(guard);
            assert_eq!(Span::current().id(), span_c.id());
        });
    }

    #[test]
    fn pause_until_no_match_returns_none_and_leaves_stack_unchanged() {
        tracing::subscriber::with_default(tracing_subscriber::registry(), || {
            let span_a = info_span!(target: "aws_sdk_x::operation::y", "A");
            let _enter_a = span_a.enter();
            let span_c = info_span!("C");
            let _enter_c = span_c.enter();

            let result = SpanPauser::pause_until(|s| {
                s.metadata()
                    .map(|m| m.name() == "never_matches")
                    .unwrap_or(false)
            });

            assert!(result.is_none());
            assert_eq!(Span::current().id(), span_c.id());
        });
    }

    #[test]
    fn pause_until_with_no_entered_span_returns_none() {
        tracing::subscriber::with_default(tracing_subscriber::registry(), || {
            let result = SpanPauser::pause_until(|_| true);
            assert!(result.is_none());
        });
    }
}
