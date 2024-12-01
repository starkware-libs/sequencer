use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use rstest::rstest;
use tempfile::NamedTempFile;

use crate::resource_limits::ResourceLimits;
use crate::test_utils::{get_memory_usage_kb, get_xmalloc_error_num_bytes};

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
    command.stderr(std::process::Stdio::piped());
    memory_size_rlimit.apply(&mut command);
    let mut child_process = command.spawn().expect("Failed to start memory consuming process");

    let child_stderr = child_process.stderr.take().unwrap();
    let pid = child_process.id();
    let mut last_memory_usage_bytes = 0;

    loop {
        // Check if the child has exited.
        if let Ok(Some(status)) = child_process.try_wait() {
            println!("Child exited with status: {}", status);
            let failed_allocation_bytes = get_xmalloc_error_num_bytes(child_stderr).unwrap();
            println!("Child process failed to allocate {} bytes.", failed_allocation_bytes);
            // Check that the used memory plus failed allocation is greater than the limit.
            assert!(last_memory_usage_bytes + failed_allocation_bytes >= memory_limit);
            break;
        }

        // The child is still running, check its memory usage.
        let memory_usage_kb = get_memory_usage_kb(pid).unwrap();
        println!("Child process is using {} KB of memory.", memory_usage_kb);
        last_memory_usage_bytes = memory_usage_kb * 1024;
        assert!(last_memory_usage_bytes < memory_limit);
        thread::sleep(Duration::from_millis(250));
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
