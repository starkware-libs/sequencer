use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum UpdateStrategy {
    Recreate,
    RollingUpdate,
}
