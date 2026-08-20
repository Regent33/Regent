//! Make the packaged voice server find its own shared libraries.
//!
//! sherpa-rs links `sherpa-onnx-c-api` DYNAMICALLY (its `static` feature is
//! mutually exclusive with `download-binaries`, which is what lets us build
//! without cmake — see ADR-029). So the binary needs `libsherpa-onnx-c-api` and
//! `libonnxruntime` at RUNTIME, and the release archive now ships them beside
//! it. Windows resolves that automatically: the directory of the .exe is on the
//! DLL search path. Linux and macOS do not — an ELF/Mach-O binary searches the
//! system paths, and sherpa-rs-sys leaves its own rpath emission commented out
//! (build.rs: "TODO: add rpath ... so it can find its dependencies in the same
//! directory of executable"). Without the lines below the shipped binary dies
//! at exec with "error while loading shared libraries", which is exactly the
//! Windows failure this fixes — there the symptom was exit 0xC0000135, silent
//! and with no message at all.
//!
//! `$ORIGIN` / `@loader_path` mean "the directory this binary is in", resolved
//! at load time, so it works from an install dir, a repo build, or a tarball
//! extracted anywhere — no LD_LIBRARY_PATH at any spawn site.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target_os.as_str() {
        // Single quotes keep the shell that some linkers invoke from expanding
        // $ORIGIN; -z origin marks the object as needing origin processing.
        "linux" => {
            println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
            println!("cargo:rustc-link-arg=-Wl,-z,origin");
        }
        "macos" => println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path"),
        _ => {}
    }
}
