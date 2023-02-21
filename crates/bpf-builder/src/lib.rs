use std::{env, path::PathBuf, process::Command, string::String};

use anyhow::{bail, Context};

static CLANG_DEFAULT: &str = "clang";
static LLVM_STRIP: &str = "llvm-strip";
static INCLUDE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/include");

// Given the filename of an eBPF program source code, compile it to OUT_DIR.
// We'll build two versions:
// - `probe_full.bpf.o`: will contain the full version
// - `probe_noloop.bpf.o`: will contain a version with the NOLOOP constant
//   defined. This version should be loaded on kernel < 5.13, where taking
//   the address of a static function would result in a verifier error.
//   See
//   - https://github.com/Exein-io/pulsar/issues/158
//   - https://github.com/torvalds/linux/commit/69c087ba6225b574afb6e505b72cb75242a3d844
pub fn build(probe: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed={probe}");
    println!("cargo:rerun-if-changed={INCLUDE_PATH}/common.bpf.h");
    println!("cargo:rerun-if-changed={INCLUDE_PATH}/buffer.bpf.h");
    println!("cargo:rerun-if-changed={INCLUDE_PATH}/output.bpf.h");
    println!("cargo:rerun-if-changed={INCLUDE_PATH}/loop.bpf.h");

    let out_path = PathBuf::from(env::var("OUT_DIR")?);

    compile(probe, out_path.join("probe_full.bpf.o"), &[])
        .context("Error compiling full version")?;
    compile(probe, out_path.join("probe_noloop.bpf.o"), &["-DNOLOOP"])
        .context("Error compiling no-loop version")?;

    Ok(())
}

fn compile(probe: &str, out_object: PathBuf, extra_args: &[&str]) -> anyhow::Result<()> {
    let clang = env::var("CLANG").unwrap_or_else(|_| String::from(CLANG_DEFAULT));
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let include_path = PathBuf::from(INCLUDE_PATH);
    let status = Command::new(clang)
        .arg(format!("-I{}", include_path.to_string_lossy()))
        .arg(format!("-I{}", include_path.join(&arch).to_string_lossy()))
        .arg("-g")
        .arg("-O2")
        .args(["-target", "bpf"])
        .arg("-c")
        .arg("-Werror")
        .arg(format!(
            "-D__TARGET_ARCH_{}",
            match arch.as_str() {
                "x86_64" => "x86".to_string(),
                "aarch64" => "arm64".to_string(),
                "riscv64" => "riscv".to_string(),
                _ => arch.clone(),
            }
        ))
        .args(extra_args)
        .arg(probe)
        .arg("-o")
        .arg(&out_object)
        .status()
        .context("Failed to execute clang")?;

    if !status.success() {
        bail!("Failed to compile eBPF program");
    }

    // Strip debug symbols
    let status = Command::new(LLVM_STRIP)
        .arg("-g")
        .arg(out_object)
        .status()
        .context("Failed to execute llvm-strip")?;

    if !status.success() {
        bail!("Failed strip eBPF program");
    }

    Ok(())
}