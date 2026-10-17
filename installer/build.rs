fn main() {
    let exe = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/release/zenbruno.exe");
    if !exe.exists() {
        panic!("{} not found - run `cargo build --release` in the ZenBruno folder first", exe.display());
    }
    println!("cargo:rerun-if-changed={}", exe.display());
}
