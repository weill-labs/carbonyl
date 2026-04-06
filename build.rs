use std::{env, path::PathBuf};

fn chromium_src() -> PathBuf {
    env::var_os("CHROMIUM_SRC")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./chromium/src"))
}

#[cfg(target_arch = "x86_64")]
fn link_sysroot() {
    let chromium_src = chromium_src();
    let sysroot_path = chromium_src.join("build/linux/debian_bullseye_amd64-sysroot");

    if sysroot_path.is_dir() {
        println!(
            "cargo:rustc-link-search={}",
            sysroot_path.join("lib/x86_64-linux-gnu").display()
        );
        println!(
            "cargo:rustc-link-search={}",
            sysroot_path.join("usr/lib/x86_64-linux-gnu").display()
        );

        println!("cargo:rustc-link-arg=--sysroot={}", sysroot_path.display());
    } else {
        println!("cargo:warning={}", "x86_64 debian sysroot provided by chromium was not found!");
        println!("cargo:warning={}", "carbonyl may fail to link against a proper libc!");
    }
}

#[cfg(target_arch = "x86")]
fn link_sysroot() {
    let chromium_src = chromium_src();
    let sysroot_path = chromium_src.join("build/linux/debian_bullseye_i386-sysroot");

    if sysroot_path.is_dir() {
        println!(
            "cargo:rustc-link-search={}",
            sysroot_path.join("lib/i386-linux-gnu").display()
        );
        println!(
            "cargo:rustc-link-search={}",
            sysroot_path.join("usr/lib/i386-linux-gnu").display()
        );

        println!("cargo:rustc-link-arg=--sysroot={}", sysroot_path.display());
    } else {
        println!("cargo:warning={}", "x86 debian sysroot provided by chromium was not found!");
        println!("cargo:warning={}", "carbonyl may fail to link against a proper libc!");
    }
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "x86")))]
fn link_sysroot() {
    // Intentionally left blank.
}

fn main() {
    link_sysroot();
}
