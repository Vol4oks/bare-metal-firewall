fn main() {
    println!("cargo::rerun-if-changed=../linker.ld");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("none") {
        let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        println!("cargo::rustc-link-arg=-T{root}/../linker.ld");
    }
}
