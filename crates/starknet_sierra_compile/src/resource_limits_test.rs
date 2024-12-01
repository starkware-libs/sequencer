use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use rstest::rstest;
use sysinfo::{Pid, System};
use tempfile::NamedTempFile;

use crate::resource_limits::ResourceLimits;

#[rstest]
fn test_cpu_time_limit() {
    let cpu_limit = 1; // 1 second
    let cpu_time_rlimit = ResourceLimits::new(Some(cpu_limit), None, None);

    let start = Instant::now();
    let mut command = Command::new("bash");
    command.args(["-c", "while true; do :; done;"]);
    cpu_time_rlimit.apply(&mut command);
    command.spawn().expect("Failed to start CPU consuming process").wait().unwrap();
    assert!(start.elapsed().as_secs() <= cpu_limit);
}

#[rstest]
fn test_memory_size_limit() {
    let memory_limit = 9 * 512 * 1024; // 4.5 MB
    let memory_size_rlimit = ResourceLimits::new(None, None, Some(memory_limit));

    let mut command = Command::new("bash");
    command.args(["-c", "a=(); while true; do a+=0; done;"]);
    memory_size_rlimit.apply(&mut command);
    let mut child_process = command.spawn().expect("Failed to start memory consuming process");

    let pid = Pid::from_u32(child_process.id());

    let mut system = System::new_all();
    let mut last_memory_measurement: Option<u64> = None;

    loop {
        // Refresh sysinfo's process list.
        system.refresh_all();

        // Check if the child has exited.
        if let Ok(Some(status)) = child_process.try_wait() {
            println!("Child exited with status: {}", status);
            println!("Last known memory usage: {} bytes.", last_memory_measurement.unwrap_or_default());
            assert!(last_memory_measurement.unwrap_or_default() < memory_limit);
            break;
        }

        // The child is still running, check its memory usage.
        if let Some(process_info) = system.process(pid) {
            let memory_usage = process_info.virtual_memory();
            last_memory_measurement = Some(memory_usage);
            println!("Child process is using {} bytes of memory.", memory_usage);
            assert!(last_memory_measurement.unwrap_or_default() < memory_limit);
        } else {
            // If the child process is missing in sysinfo but try_wait() still says None,
            // there's a small timing gap. Just keep looping or break out here.
            println!("Child process not found. Maybe it just exited.");
        }

        thread::sleep(Duration::from_secs(1));
    }
}

#[rstest]
fn test_file_size_limit() {
    let file_limit = 10; // 10 bytes
    let file_size_rlimit = ResourceLimits::new(None, Some(file_limit), None);
    let temp_file = NamedTempFile::new().expect("Failed to create temporary file");
    let temp_file_path = temp_file.path().to_str().unwrap();

    let mut command = Command::new("bash");
    command.args(["-c", format!("while true; do echo 0 >> {temp_file_path}; done;").as_str()]);
    file_size_rlimit.apply(&mut command);
    command.spawn().expect("Failed to start disk consuming process").wait().unwrap();
    assert_eq!(std::fs::metadata(temp_file_path).unwrap().len(), file_limit);
}

#[rstest]
fn test_successful_resource_limited_command() {
    let print_message = "Hello World!";

    let cpu_limit = Some(1); // 1 second
    let file_limit = Some(u64::try_from(print_message.len()).unwrap() + 1);
    let memory_limit = Some(5 * 1024 * 1024); // 5 MB
    let resource_limits = ResourceLimits::new(cpu_limit, file_limit, memory_limit);

    let temp_file = NamedTempFile::new().expect("Failed to create temporary file");
    let temp_file_path = temp_file.path().to_str().unwrap();

    let mut command = Command::new("bash");
    command.args(["-c", format!("echo '{print_message}' > {temp_file_path}").as_str()]);
    resource_limits.apply(&mut command);
    let exit_status = command.spawn().expect("Failed to start process").wait().unwrap();

    assert!(exit_status.success());
    assert_eq!(std::fs::read_to_string(temp_file_path).unwrap(), format!("{print_message}\n"));
}
