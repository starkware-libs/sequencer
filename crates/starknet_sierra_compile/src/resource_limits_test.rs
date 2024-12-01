use std::process::Command;
use std::time::Instant;

use rstest::rstest;

use crate::resource_limits::ResourcesLimits;

#[rstest]
fn test_cpu_time_limit() {
    let cpu_time_limits = ResourcesLimits::new(Some(1), None, None);

    let start = Instant::now();
    let mut command = Command::new("bash");
    command.args(["-c", "while true; do :; done;"]);
    cpu_time_limits.apply(&mut command);
    command.spawn().expect("Failed to start CPU consuming process").wait().unwrap();
    assert!(start.elapsed().as_secs() <= 1);
}

#[rstest]
fn test_memory_size_limit() {
    let memory_size_limits = ResourcesLimits::new(None, None, Some(100000));

    let mut command = Command::new("bash");
    command.args(["-c", "a=(); while true; do a+=0; done;"]);
    memory_size_limits.apply(&mut command);
    command.spawn().expect("Failed to start memory consuming process").wait().unwrap();
}

#[rstest]
fn test_file_size_limit() {
    let file_size_limits = ResourcesLimits::new(None, Some(100), None);

    let mut command = Command::new("bash");
    command.args(["-c", "echo 0 > /tmp/file.txt; while true; do echo 0 >> /tmp/file.txt; done;"]);
    file_size_limits.apply(&mut command);
    command.spawn().expect("Failed to start disk consuming process").wait().unwrap();
    assert!(std::fs::metadata("/tmp/file.txt").unwrap().len() <= 100);
    std::fs::remove_file("/tmp/file.txt").unwrap();
}
