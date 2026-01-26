fn main() {
    println!("cargo:rustc-link-search=native=target/aarch64-apple-darwin/release");
    println!("cargo:rustc-link-lib=dylib=demo_lib");
    println!("cargo:rustc-link-arg=-Wl,-bind_at_load");
}
