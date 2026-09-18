//! Compiles the native kernels.
//!
//! The assembly is written for aarch64 NEON. On any other architecture the
//! `.asm` files are simply not built and the portable Rust path is used
//! instead, so the crate still compiles and still produces identical results —
//! just more slowly.

fn main() {
    println!("cargo:rerun-if-changed=native");

    // C and C++ build everywhere.
    cc::Build::new()
        .file("native/resample.c")
        .file("native/grain.c")
        .opt_level(3)
        .warnings(true)
        .compile("detends_c");

    cc::Build::new()
        .cpp(true)
        .file("native/synth.cpp")
        .opt_level(3)
        .warnings(true)
        .compile("detends_cxx");

    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if arch == "aarch64" {
        // clang does not infer a language from the `.asm` extension — it is a
        // NASM convention, not a Unix one — so the language is named outright.
        // `assembler-with-cpp` also gets us the preprocessor, which the symbol
        // naming below needs.
        cc::Build::new()
            .file("native/mix_neon.asm")
            .file("native/convert_neon.asm")
            .file("native/analyse_neon.asm")
            .flag("-x")
            .flag("assembler-with-cpp")
            .compile("detends_asm");

        println!("cargo:rustc-cfg=detends_neon");
    }

    println!("cargo:rustc-check-cfg=cfg(detends_neon)");
}
