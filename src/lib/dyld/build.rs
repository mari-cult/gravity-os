fn main() {
    println!("cargo:rustc-link-arg=-Wl,-e,_dyld_start");
}
