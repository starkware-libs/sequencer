use crate::errors::ConfigManagerError;

pub type ConfigManagerResult<T> = Result<T, ConfigManagerError>;
