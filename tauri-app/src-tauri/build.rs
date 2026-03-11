fn main() {
    // Pass target triple to the binary for sidecar resolution
    println!("cargo:rustc-env=TARGET_TRIPLE={}", std::env::var("TARGET").unwrap());
    tauri_build::build();
}
