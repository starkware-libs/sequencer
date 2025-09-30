use tokio::sync::mpsc;

pub enum TrySendResult {
    Ok,
    Full,
    Closed,
}

pub fn try_send_or_closed<T>(sender: &mpsc::Sender<T>, message: T) -> TrySendResult {
    match sender.try_send(message) {
        Ok(()) => TrySendResult::Ok,
        Err(mpsc::error::TrySendError::Full(_)) => TrySendResult::Full,
        Err(mpsc::error::TrySendError::Closed(_)) => TrySendResult::Closed,
    }
}

pub fn try_send_or_exit<T>(sender: &mpsc::Sender<T>, message: T, channel_name: &str) -> bool {
    match try_send_or_closed(sender, message) {
        TrySendResult::Ok => true,
        TrySendResult::Full => {
            tracing::error!(
                "Backpressure detected: {} channel is full! Task will exit.",
                channel_name
            );
            false
        }
        TrySendResult::Closed => {
            tracing::trace!("{} channel closed, exiting gracefully", channel_name);
            false
        }
    }
}

pub fn try_send_critical<T>(sender: &mpsc::Sender<T>, message: T, channel_name: &str) -> bool {
    match try_send_or_closed(sender, message) {
        TrySendResult::Ok => true,
        TrySendResult::Full => {
            tracing::error!("CRITICAL: {} channel is full (backpressure detected)!", channel_name);
            false
        }
        TrySendResult::Closed => {
            tracing::error!(
                "CRITICAL: {} channel closed unexpectedly - receiver task died!",
                channel_name
            );
            false
        }
    }
}
