use tokio::task::AbortHandle;

/// Aborts all tracked `spawn_blocking` tasks on drop so that a `tokio::time::timeout`
/// cancellation doesn't silently leak running blocking threads.
#[derive(Default)]
pub(crate) struct BlockingTaskTracker {
    handles: Vec<AbortHandle>,
}

impl BlockingTaskTracker {
    pub fn track(&mut self, handle: AbortHandle) {
        self.handles.push(handle);
    }
}

impl Drop for BlockingTaskTracker {
    fn drop(&mut self) {
        for h in &self.handles {
            h.abort();
        }
    }
}
