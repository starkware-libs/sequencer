//! Utilities for working with mpsc channels.

use std::fmt;

use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy)]
pub(crate) enum ChannelName {
    BehaviourToCore,
}

impl fmt::Display for ChannelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChannelName::BehaviourToCore => write!(f, "Behaviour->Core"),
        }
    }
}

/// Use this if you expect the receiver to never drop the channel.
///
/// Returns Ok(()) if the message was sent successfully, or Err(failed_message) if the channel is
/// closed.
pub(crate) async fn send_non_critical<T>(
    sender: &mpsc::Sender<T>,
    message: T,
    channel_name: ChannelName,
) -> Result<(), T> {
    let failed_message = match sender.try_send(message) {
        Ok(()) => return Ok(()),
        Err(mpsc::error::TrySendError::Full(failed_message)) => failed_message,
        Err(mpsc::error::TrySendError::Closed(_failed_message)) => {
            tracing::trace!("{} channel closed, exiting gracefully", channel_name);
            return Err(_failed_message);
        }
    };

    tracing::warn!("Backpressure on {} channel, awaiting send", channel_name);

    if let Err(failed_message) = sender.send(failed_message).await {
        tracing::trace!("{} channel closed, exiting gracefully", channel_name);
        return Err(failed_message.0);
    }

    Ok(())
}
