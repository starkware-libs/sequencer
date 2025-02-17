use async_trait::async_trait;

use crate::errors::ReplaceComponentError;

#[async_trait]
pub trait ComponentServerStarter {
    async fn start(&mut self) -> ();
}

pub trait ComponentReplacer<Component> {
    fn replace(&mut self, component: Component) -> Result<(), ReplaceComponentError>;
}
