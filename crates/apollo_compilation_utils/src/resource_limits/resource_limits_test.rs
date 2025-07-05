use std::os::unix::process::ExitStatusExt;
use std::process::Command;
use std::time::Instant;

use rstest::rstest;
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
    let status = command.spawn().expect("Failed to start CPU consuming process").wait().unwrap();
    assert!(start.elapsed().as_secs() <= cpu_limit);
    let signal = status.signal();
    assert_eq!(signal, Some(9), "Process should terminate with SIGKILL (9) got {signal:?}");
}

#[rstest]
fn test_memory_size_limit() {
    let memory_limit = 9 * 512 * 1024; // 4.5 MB
    let memory_size_rlimit = ResourceLimits::new(None, None, Some(memory_limit));

    let mut command = Command::new("bash");
    command.args(["-c", "a=(); while true; do a+=0; done;"]);
    command.stderr(std::process::Stdio::piped());
    memory_size_rlimit.apply(&mut command);
    let output = command.output().expect("Failed to start memory consuming process");

    let signal = output.status.signal();
    assert!(signal.is_none(), "Exceeding memory usage should not cause a signal, got {signal:?}");

    let stderr = String::from_utf8_lossy(&output.stderr);

    for line in stderr.lines() {
        if line.starts_with("bash: xmalloc: cannot allocate") {
            println!(
                "Child process exited with status code: {}, and the following memory allocation \
                 error:\n {}.",
                output.status.code().unwrap(),
                line
            );
            return;
        }
    }

    panic!("Child process did not exit with a memory allocation error.");
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
    let status = command.spawn().expect("Failed to start disk consuming process").wait().unwrap();
    assert_eq!(std::fs::metadata(temp_file_path).unwrap().len(), file_limit);
    let signal = status.signal();
    assert!(signal == Some(25), "Process should terminate with SIGXFSZ (25), got {signal:?}");
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
    assert!(
        exit_status.success(),
        "Process did not complete successfully: signal={:?}",
        exit_status.signal()
    );
    assert_eq!(std::fs::read_to_string(temp_file_path).unwrap(), format!("{print_message}\n"));
}
