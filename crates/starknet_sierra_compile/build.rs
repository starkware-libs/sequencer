use std::path::Path;
use std::process::Command;
use std::{env, fs};

fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");

    download_cairo();
}

/// Downloads the Cairo crate from StarkWares release page and extracts its contents into the
/// `target` directory. This crate includes the `starknet-sierra-compile` binary, which is used to
/// compile Sierra to Casm. In this crate (`starknet_sierra_compile`), the binary is executed as a
/// subprocess whenever Sierra compilation is required.
fn download_cairo() {
    let out_dir = env::var("OUT_DIR").expect("Failed to get the OUT_DIR environment variable");
    // Navigate from this crate's build folder to reach the `target/BUILD_FLAVOR` directory.
    let target_dir = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("Failed to navigate up three levels from OUT_DIR");
    let cairo_folder_dir = target_dir.join("cairo");

    if cairo_folder_dir.exists() {
        println!(
            "The cairo crate already exists in the target directory: {:?}",
            cairo_folder_dir.display()
        );
        return;
    }

    let url = "https://github.com/starkware-libs/cairo/releases/download/v2.7.0-rc.1/release-x86_64-unknown-linux-musl.tar.gz";
    let download_path = target_dir.join("release-x86_64-unknown-linux-musl.tar.gz");

    // TODO(Arni): Consider using use reqwest::blocking::get; instead of wget. Then consider using
    // the tar crate to extract the tar.gz file.
    let wget_command_execution_status = Command::new("wget")
        .arg("-O")
        .arg(&download_path)
        .arg(url)
        .current_dir(target_dir)
        .status()
        .expect("Failed to execute wget command");

    if !wget_command_execution_status.success() {
        panic!("Failed to download the cairo tar");
    }

    println!("Successfully downloaded the cairo tar");

    let tar_command_execution_status = Command::new("tar")
        .arg("-xvf")
        .arg(&download_path)
        .current_dir(target_dir)
        .status()
        .expect("Failed to execute tar command");

    // Clean up the downloaded file.
    fs::remove_file(&download_path).expect("Failed to remove the downloaded file");

    println!("Successfully removed the downloaded tar.gz file");

    // Assert the unzip command was succesful.
    if !tar_command_execution_status.success() {
        panic!("Failed to extract the cairo crate from zip");
    }

    assert!(cairo_folder_dir.exists(), "cairo folder was not created as expected.");

    println!(
        "Successfully extracted the cairo crate into the target directory: {:?}",
        cairo_folder_dir.display()
    );
}
