use std::io;
use std::process::Command;

/// A struct to hold resource limits for a process.
/// Each limit is optional and can be set to `None` if not needed.
/// NOTE: This is a trivial implementation for compiling on windows.
pub struct ResourceLimits;

impl ResourceLimits {
    pub fn new(
        _cpu_time: Option<u64>,
        _file_size: Option<u64>,
        _memory_size: Option<u64>,
    ) -> ResourceLimits {
        ResourceLimits {}
    }

    pub fn set(&self) -> io::Result<()> {
        Ok(())
    }

    /// Apply the resource limits to a given command object. This moves the [`ResourceLimits`]
    /// struct into a closure that is held by the given command. The closure is executed in the
    /// child process spawned by the command, right before it invokes the `exec` system call.
    pub fn apply(self, command: &mut Command) -> &mut Command {
        command
    }
}
