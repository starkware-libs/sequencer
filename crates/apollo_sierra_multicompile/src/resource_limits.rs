#[cfg(unix)]
mod resource_limits_unix;
#[cfg(unix)]
pub use resource_limits_unix::ResourceLimits;

#[cfg(windows)]
mod resource_limits_windows;
#[cfg(windows)]
pub use resource_limits_windows::ResourceLimits;
