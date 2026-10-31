fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") && std::path::Path::new("icon.ico").exists() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icon.ico");
        res.compile().unwrap();
    }
    println!("cargo:rerun-if-changed=icon.ico");
}
