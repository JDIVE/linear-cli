fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        // The CLI command tree is large enough to overflow the default 1MB stack
        // on Windows during startup/help parsing.
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }
}
