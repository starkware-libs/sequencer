use async_trait::async_trait;

use crate::errors::{ComponentServerError, ReplaceComponentError};

#[async_trait]
pub trait ComponentServerStarter {
    async fn start(&mut self) -> Result<(), ComponentServerError>;
}

pub trait ComponentReplacer<Component> {
    fn replace(&mut self, component: Component) -> Result<(), ReplaceComponentError>;
}
