//! Trace context propagation utilities for cross-component tracing.
//!
//! This module provides utilities for propagating OpenTelemetry trace context
//! across component boundaries using W3C Trace Context headers.

use std::collections::HashMap;

use opentelemetry::propagation::TextMapPropagator;
use opentelemetry::trace::TraceContextExt;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Header name for W3C Trace Context traceparent header.
pub const TRACEPARENT_HEADER: &str = "traceparent";

/// Extracts the current trace context and formats it as a W3C traceparent header value.
/// Returns None if there is no active span or trace context.
pub fn get_traceparent() -> Option<String> {
    let context = Span::current().context();
    let mut carrier = HashMap::new();
    TraceContextPropagator::new().inject_context(&context, &mut carrier);
    carrier.remove(TRACEPARENT_HEADER)
}

/// Parses a W3C traceparent header value and sets it as the parent context for the current span.
pub fn set_parent_context_from_traceparent(traceparent: &str) {
    let carrier = HashMap::from([(TRACEPARENT_HEADER.to_string(), traceparent.to_string())]);
    let parent_context = TraceContextPropagator::new().extract(&carrier);
    Span::current().set_parent(parent_context);
}

/// Gets the current trace_id as a hex string, if available.
pub fn get_current_trace_id() -> Option<String> {
    let span_context = Span::current().context().span().span_context().clone();
    if span_context.is_valid() { Some(span_context.trace_id().to_string()) } else { None }
}
