//! Utilities for working with mpsc channels.

use std::fmt;

use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy)]
pub(crate) enum ChannelName {
    BehaviourToCore,
    BroadcasterToCore,
    CoreToBehaviour,
    CoreToValidator,
    StateManagerToCore,
    StateManagerToValidator,
    ValidatorToStateManager,
}

impl fmt::Display for ChannelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChannelName::BehaviourToCore => write!(f, "Behaviour->Core"),
            ChannelName::BroadcasterToCore => write!(f, "Broadcaster->Core"),
            ChannelName::CoreToBehaviour => write!(f, "Core->Behaviour"),
            ChannelName::CoreToValidator => write!(f, "Core->Validator"),
            ChannelName::StateManagerToCore => write!(f, "StateManager->Core"),
            ChannelName::StateManagerToValidator => write!(f, "StateManager->Validator"),
            ChannelName::ValidatorToStateManager => write!(f, "Validator->StateManager"),
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

/// Use this if the receiver might drop the channel (or the receiver is not guaranteed to be alive)
///
/// Panics if the channel is closed.
pub(crate) async fn send_critical<T>(
    sender: &mpsc::Sender<T>,
    message: T,
    channel_name: ChannelName,
) {
    if let Err(_message) = send_non_critical(sender, message, channel_name).await {
        let error_message =
            format!("CRITICAL: {} channel closed unexpectedly - receiver task died!", channel_name);
        tracing::error!("{}", error_message);
        panic!("{}", error_message);
    }
}

/// Use this if the receiver might drop the channel (or the receiver is not guaranteed to be alive)
///
/// Panics if the channel is closed.
pub(crate) fn try_send_critical<T>(
    sender: &mpsc::Sender<T>,
    message: T,
    channel_name: ChannelName,
) {
    let error_message = match sender.try_send(message) {
        Ok(()) => return,
        Err(mpsc::error::TrySendError::Full(_failed_message)) => {
            format!("CRITICAL: {} channel backpressure - message dropped!", channel_name)
        }
        Err(mpsc::error::TrySendError::Closed(_failed_message)) => {
            format!("CRITICAL: {} channel closed unexpectedly - receiver task died!", channel_name)
        }
    };
    tracing::error!("{}", error_message);
    panic!("{}", error_message);
}
