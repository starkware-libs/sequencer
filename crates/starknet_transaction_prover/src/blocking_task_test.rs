use tracing::{info, info_span};
use tracing_test::traced_test;

use super::spawn_blocking_in_current_span;

/// The task is spawned inside the span but awaited after leaving it, so the field can only reach
/// the log line through the span captured at spawn time. `logs_assert` only sees lines that carry
/// the test's own span, so a lost span also hides the line from it.
#[tokio::test]
#[traced_test]
async fn blocking_task_logs_carry_enclosing_span_fields() {
    let request_span = info_span!("http_request", request_id = %"blocking-task-request-id");
    let blocking_task = request_span
        .in_scope(|| spawn_blocking_in_current_span(|| info!("Inside the blocking task")));
    blocking_task.await.expect("the blocking task must not panic");

    logs_assert(|lines: &[&str]| {
        let task_line = lines
            .iter()
            .find(|line| line.contains("Inside the blocking task"))
            .ok_or("no blocking task log line carries the enclosing spans")?;
        if task_line.contains("request_id=blocking-task-request-id") {
            Ok(())
        } else {
            Err(format!("the blocking task log line must carry the span field: {task_line}"))
        }
    });
}
