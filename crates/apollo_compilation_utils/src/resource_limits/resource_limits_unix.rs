use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

use rlimit::{setrlimit, Resource};

#[cfg(test)]
#[path = "resource_limits_test.rs"]
pub mod test;

/// A struct to hold the information needed to set a limit on an individual OS resource.
struct RLimit {
    /// A kind of resource. All resource constants are available on all unix platforms.
    /// See <https://docs.rs/rlimit/latest/rlimit/struct.Resource.html> for more information.
    resource: Resource,
    /// The soft limit for the resource. The limit may be increased by the process being limited.
    soft_limit: u64,
    /// The hard limit for the resource. May not be increased or exceeded by the affected process.
    hard_limit: u64,
    /// The units by which the resource is measured.
    units: String,
}

impl RLimit {
    /// Set the resource limit for the current process.
    fn set(&self) -> io::Result<()> {
        // Use `eprintln!` and not a logger because this method is called in an unsafe block, and we
        // don't want to risk unexpected behavior. Use 'eprintln!' and not 'println!' because it
        // corrupts stdout which is deserialized later.
        eprintln!(
            "Setting {:?} limits: {} {} soft limit; {} {} hard limit.",
            self.resource, self.soft_limit, self.units, self.hard_limit, self.units
        );
        setrlimit(self.resource, self.soft_limit, self.hard_limit)
    }
}

/// A struct to hold resource limits for a process.
/// Each limit is optional and can be set to `None` if not needed.
/// NOTE: This struct is fully implemented only for Unix-like systems.
pub struct ResourceLimits {
    /// A limit (in seconds) on the amount of CPU time that the process can consume.
    cpu_time: Option<RLimit>,
    /// The maximum size (in bytes) of files that the process may create.
    file_size: Option<RLimit>,
    /// The maximum size (in bytes) of the process’s virtual memory (address space).
    memory_size: Option<RLimit>,
}

impl ResourceLimits {
    pub fn new(
        cpu_time: Option<u64>,
        file_size: Option<u64>,
        memory_size: Option<u64>,
    ) -> ResourceLimits {
        ResourceLimits {
            cpu_time: cpu_time.map(|t| RLimit {
                resource: Resource::CPU,
                soft_limit: t,
                hard_limit: t,
                units: "seconds".to_string(),
            }),
            file_size: file_size.map(|x| RLimit {
                resource: Resource::FSIZE,
                soft_limit: x,
                hard_limit: x,
                units: "bytes".to_string(),
            }),
            memory_size: memory_size.map(|y| RLimit {
                resource: Resource::AS,
                soft_limit: y,
                hard_limit: y,
                units: "bytes".to_string(),
            }),
        }
    }

    /// Set all defined resource limits for the current process. Limits set to `None` are ignored.
    pub fn set(&self) -> io::Result<()> {
        [self.cpu_time.as_ref(), self.file_size.as_ref(), self.memory_size.as_ref()]
            .iter()
            .flatten()
            .try_for_each(|resource_limit| resource_limit.set())
    }

    /// Apply the resource limits to a given command object. This moves the [`ResourceLimits`]
    /// struct into a closure that is held by the given command. The closure is executed in the
    /// child process spawned by the command, right before it invokes the `exec` system call.
    pub fn apply(self, command: &mut Command) -> &mut Command {
        if self.cpu_time.is_none() && self.file_size.is_none() && self.memory_size.is_none() {
            return command;
        }
        unsafe {
            // The `pre_exec` method runs a given closure after the parent process has been forked
            // but before the child process calls `exec`.
            //
            // This closure runs in the child process after a `fork`, which primarily means that any
            // modifications made to memory on behalf of this closure will **not** be visible to the
            // parent process. This environment is often very constrained. Normal operations--such
            // as using `malloc`, accessing environment variables through [`std::env`] or acquiring
            // a mutex--are not guaranteed to work, because after `fork`, only one thread exists in
            // the child process, while there may be multiple threads in the parent process.
            //
            // This closure is considered safe for the following reasons:
            // 1. The [`ResourceLimits`] struct is fully constructed and moved into the closure.
            // 2. No heap allocations occur in the `set` method.
            // 3. `setrlimit` is an async-signal-safe system call, which means it is safe to invoke
            //   after `fork`. This is established in the POSIX `fork` specification:
            //   > ... the child process may only execute async-signal-safe operations until such
            //    time as one of the `exec` functions is called.
            //   (See <https://pubs.opengroup.org/onlinepubs/9699919799/functions/fork.html>)
            command.pre_exec(move || self.set())
        }
    }
}
