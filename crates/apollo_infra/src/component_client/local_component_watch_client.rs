use tokio::sync::watch::Receiver;

use crate::component_definitions::ComponentChannelClient;

/// A local client that reads the latest value from a [`tokio::sync::watch`]
/// channel.
#[derive(Clone)]
pub struct LocalComponentWatchClient<InfoSource>
where
    InfoSource: Send + Sync + Clone,
{
    info_source_rx: Receiver<InfoSource>,
}

impl<InfoSource> LocalComponentWatchClient<InfoSource>
where
    InfoSource: Send + Sync + Clone,
{
    pub fn new(info_source_rx: Receiver<InfoSource>) -> Self {
        Self { info_source_rx }
    }
}

impl<InfoSource> ComponentChannelClient<InfoSource> for LocalComponentWatchClient<InfoSource>
where
    InfoSource: Send + Sync + Clone,
{
    fn get_info(&self) -> InfoSource {
        // `borrow()` returns a reference to the value owned by the channel, hence we clone it.
        self.info_source_rx.borrow().clone()
    }
}
