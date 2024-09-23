use std::any::type_name;

use async_trait::async_trait;
use tracing::info;

use crate::errors::ComponentError;

#[async_trait]
pub trait Startable<StartError> {
    async fn start(&mut self) -> Result<(), StartError> {
        info!("Starting component (default trait impl).");
        Ok(())
    }
}

pub trait DefaultComponentStarter {}

#[async_trait]
impl<T: Send + Sync> Startable<ComponentError> for T
where
    T: DefaultComponentStarter,
{
    async fn start(&mut self) -> Result<(), ComponentError> {
        info!("Component {} received request", type_name::<T>());
        info!("Starting component (default impl for T).");
        Ok(())
    }
}

pub struct StartOnce<T> {
    inner: T,
    started: bool,
}

impl<T> StartOnce<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, started: false }
    }
}

// #[async_trait]
// impl<T, StartError > Startable<StartError> for StartOnce<T> where
// T:  Send + Sync +  Startable<StartError>,
// StartError: From<&'static str>,
// {
//     async fn start(&mut self) -> Result<(), StartError> {
//         if self.started {
//             return Err(StartError::from("Already started"));
//         }
//         self.started = true;
//         self.inner.start().await
//     }
// }

// impl<T, ComponentServerError > ComponentServerStarter for StartOnce<ComponentServerError>;

// #[async_trait]
// impl<T: Send + Sync + Startable<StartError>> ComponentServerStarter for StartOnce<T> {
//     async fn start(&mut self) -> Result<(), StartError> {
//         if self.started {
//             return std::error::Error
//         }
//         self.started = true;
//         self.inner.start().await
//     }
// }

// #[async_trait]
// impl<T: Send + Sync + Startable<ComponentServerError>> ComponentServerStarter for StartOnce<T> {
//     async fn start(&mut self) -> Result<(), ComponentServerError> {
//         if self.started {
//             return Err(ComponentServerError::AlreadyStarted);
//         }
//         self.started = true;
//         self.inner.start().await
//     }
// }

// #[async_trait]
// impl<T: Send + Sync + Startable<ComponentError>> ComponentStarter for StartOnce<T> {
//     async fn start(&mut self) -> Result<(), ComponentError> {
//         if self.started {
//             return Err(ComponentError::AlreadyStarted);
//         }
//         self.started = true;
//         self.inner.start().await
//     }
// }
