use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

use rlimit::{setrlimit, Resource};

#[cfg(test)]
#[path = "resource_limits_test.rs"]
pub mod test;

#[allow(dead_code)]
struct ResourceLimits {
    resource: Resource,
    soft: u64,
    hard: u64,
    units: String,
}

impl ResourceLimits {
    #[allow(dead_code)]
    fn set(&self) -> io::Result<()> {
        // Use `println!` and not a logger because this method is called in an unsafe block, and we
        // don't want to risk unexpected behavior.
        println!("Setting {:?} limits to {} {}.", self.resource, self.soft, self.units);
        setrlimit(self.resource, self.soft, self.hard)
    }
}

#[allow(dead_code)]
struct ResourcesLimits {
    cpu_time: Option<ResourceLimits>,
    file_size: Option<ResourceLimits>,
    memory_size: Option<ResourceLimits>,
}

impl ResourcesLimits {
    #[allow(dead_code)]
    fn new(
        cpu_time: Option<u64>,
        file_size: Option<u64>,
        memory_size: Option<u64>,
    ) -> ResourcesLimits {
        ResourcesLimits {
            cpu_time: cpu_time.map(|x| ResourceLimits {
                resource: Resource::CPU,
                soft: x,
                hard: x,
                units: "seconds".to_string(),
            }),
            file_size: file_size.map(|x| ResourceLimits {
                resource: Resource::FSIZE,
                soft: x,
                hard: x,
                units: "bytes".to_string(),
            }),
            memory_size: memory_size.map(|x| ResourceLimits {
                resource: Resource::AS,
                soft: x,
                hard: x,
                units: "bytes".to_string(),
            }),
        }
    }

    #[allow(dead_code)]
    fn set(&self) -> io::Result<()> {
        [self.cpu_time.as_ref(), self.file_size.as_ref(), self.memory_size.as_ref()]
            .iter()
            .flatten()
            .try_for_each(|resource_limit| resource_limit.set())
    }

    #[allow(dead_code)]
    fn apply(self, command: &mut Command) -> &mut Command {
        #[cfg(unix)]
        unsafe {
            // The `pre_exec` method runs a given closure after forking the parent process and
            // before the exec of the child process.
            //
            // This closure will be run in the context of the child process after a `fork`. This
            // primarily means that any modifications made to memory on behalf of this closure will
            // **not** be visible to the parent process. This is often a very constrained
            // environment where normal operations like `malloc`, accessing environment variables
            // through [`std::env`] or acquiring a mutex are not guaranteed to work (due to other
            // threads perhaps still running when the `fork` was run).
            //
            // The current closure is safe for the following reasons:
            // 1. The [`ResourcesLimits`] struct is fully constructed and moved into the closure.
            // 2. No heap allocations occur in the `set` method.
            // 3. `setrlimit` is an async-signal-safe system call, which means it is safe to invoke
            //   after `fork`.
            command.pre_exec(move || self.set())
        }
        #[cfg(not(unix))]
        // Not implemented for Windows.
        unimplemented!("Resource limits are not implemented for Windows.")
    }
}
