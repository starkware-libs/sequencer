use std::process::Command;
use std::time::Instant;

use rstest::rstest;

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
    let memory_limit = 100 * 1024; // 100 KB
    let memory_size_rlimit = ResourceLimits::new(None, None, Some(memory_limit));

    let mut command = Command::new("bash");
    command.args(["-c", "a=(); while true; do a+=0; done;"]);
    memory_size_rlimit.apply(&mut command);
    command.spawn().expect("Failed to start memory consuming process").wait().unwrap();
}

#[rstest]
fn test_file_size_limit() {
    let file_limit = 10; // 10 bytes
    let file_size_rlimit = ResourceLimits::new(None, Some(file_limit), None);

    let mut command = Command::new("bash");
    command.args(["-c", "echo 0 > /tmp/file.txt; while true; do echo 0 >> /tmp/file.txt; done;"]);
    file_size_rlimit.apply(&mut command);
    command.spawn().expect("Failed to start disk consuming process").wait().unwrap();
    assert_eq!(std::fs::metadata("/tmp/file.txt").unwrap().len(), file_limit);
    std::fs::remove_file("/tmp/file.txt").unwrap();
}
