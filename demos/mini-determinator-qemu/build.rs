use std::path::PathBuf;

fn main() {
    let output = PathBuf::from(
        std::env::var_os("OUT_DIR").expect("Cargo did not provide an output directory"),
    );
    let kernel = PathBuf::from(
        std::env::var_os("CARGO_BIN_FILE_KERNEL_kernel")
            .expect("Cargo did not provide the kernel artifact"),
    );
    let image = output.join("zeno-fcis-mini-determinator-uefi.img");

    bootloader::UefiBoot::new(&kernel)
        .create_disk_image(&image)
        .expect("failed to create the UEFI disk image");

    println!("cargo:rustc-env=ZENO_QEMU_UEFI_IMAGE={}", image.display());
    println!("cargo:rerun-if-changed=kernel/src/main.rs");
}
