use std::fs;
use std::process::Command;

include!("src/build_utils.rs");

fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");

    download_cairo();
}

const DOWNLOAD_URL: &str = "https://github.com/starkware-libs/cairo/releases/download/v2.7.0-rc.1";
const DOWNLOADED_FILE: &str = "release-x86_64-unknown-linux-musl.tar.gz";

/// Downloads the Cairo crate from StarkWares release page and extracts its contents into the
/// `target` directory. This crate includes the `starknet-sierra-compile` binary, which is used to
/// compile Sierra to Casm. In this crate (`starknet_sierra_compile`), the binary is executed as a
/// subprocess whenever Sierra compilation is required.
fn download_cairo() {
    let cairo_folder_dir = cairo_folder_dir();
    let url = format!("{}/{}", DOWNLOAD_URL, DOWNLOADED_FILE);
    let download_path = out_dir().join(DOWNLOADED_FILE);

    if cairo_folder_dir.exists() {
        println!(
            "The cairo crate already exists in the target directory: {:?}",
            cairo_folder_dir.display()
        );
        return;
    }

    // TODO(Arni): Consider using use reqwest::blocking::get; instead of wget. Then consider using
    // the tar crate to extract the tar.gz file.
    let wget_command_execution_status = Command::new("wget")
        .arg("-O")
        .arg(&download_path)
        .arg(url)
        .current_dir(out_dir())
        .status()
        .expect("Failed to execute wget command");

    if !wget_command_execution_status.success() {
        panic!("Failed to download the cairo tar");
    }

    println!("Successfully downloaded the cairo tar");

    let tar_command_execution_status = Command::new("tar")
        .arg("-xvf")
        .arg(&download_path)
        .current_dir(target_dir())
        .status()
        .expect("Failed to execute tar command");

    // Clean up the downloaded file.
    fs::remove_file(download_path).expect("Failed to remove the downloaded file");

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
