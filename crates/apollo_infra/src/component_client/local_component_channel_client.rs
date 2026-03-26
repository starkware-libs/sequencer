use tokio::sync::watch::Receiver;

use crate::component_definitions::ComponentChannelClient;

/// A local client that reads the latest value from a [`tokio::sync::watch`]
/// channel.
#[derive(Clone)]
pub struct LocalComponentChannelClient<T>
where
    T: Send + Sync + Clone,
{
    value_rx: Receiver<T>,
}

impl<T> LocalComponentChannelClient<T>
where
    T: Send + Sync + Clone,
{
    pub fn new(value_rx: Receiver<T>) -> Self {
        Self { value_rx }
    }
}

impl<T> ComponentChannelClient<T> for LocalComponentChannelClient<T>
where
    T: Send + Sync + Clone,
{
    fn get_value(&self) -> T {
        // `borrow()` returns a reference to the value owned by the channel, hence we clone it.
        self.value_rx.borrow().clone()
    }
}
