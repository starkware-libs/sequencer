use std::path::Path;
use std::process::Command;
use std::{env, fs};

fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");

    download_cairo();
}

/// Downloads the cairo crate from the StarkWare release page and extracts it into the target.
fn download_cairo() {
    let out_dir = env::var("OUT_DIR").expect("Failed to get the OUT_DIR environment variable");
    let out_dir = Path::new(&out_dir);
    let target_dir =
        out_dir.ancestors().nth(3).expect("Failed to navigate up three levels from OUT_DIR");
    let cairo_folder_dir = target_dir.join("cairo");

    if cairo_folder_dir.exists() {
        println!(
            "The cairo crate already exists in the target directory: {:?}",
            cairo_folder_dir.display()
        );
        return;
    }

    let url = "https://github.com/starkware-libs/cairo/releases/download/v2.7.0-rc.1/release-x86_64-unknown-linux-musl.tar.gz";
    let file_path = target_dir.join("release-x86_64-unknown-linux-musl.tar.gz");

    // TODO(Arni): Consider using use reqwest::blocking::get; instead of wget. Then consider using
    // the tar crate to extract the tar.gz file.
    let status = Command::new("wget")
        .arg("-O")
        .arg(&file_path)
        .arg(url)
        .current_dir(target_dir)
        .status()
        .expect("Failed to execute wget command");

    if !status.success() {
        panic!("Failed to download the cairo tar");
    }

    println!("Successfully downloaded the cairo tar");

    let status = Command::new("tar")
        .arg("-xvf")
        .arg(&file_path)
        .current_dir(target_dir)
        .status()
        .expect("Failed to execute tar command");

    if !status.success() {
        panic!("Failed to extract the cairo crate from zip");
    }

    println!(
        "Successfully extracted the cairo crate into the target directory: {:?}",
        cairo_folder_dir.display()
    );

    // Clean up the downloaded file
    fs::remove_file(&file_path).expect("Failed to remove the downloaded file");

    println!("Successfully removed the downloaded tar.gz file");
}
