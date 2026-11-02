fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    if !std::path::Path::new("assets/litepulse.ico").exists() {
        println!("cargo:warning=assets/litepulse.ico is missing; building without an exe icon");
        return;
    }

    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/litepulse.ico")
        .set("FileDescription", "LitePulse system monitor")
        .set("ProductName", "LitePulse")
        .set("OriginalFilename", "litepulse.exe")
        .set("FileVersion", "0.1.0.0")
        .set("ProductVersion", "0.1.0.0");
    if let Err(err) = res.compile() {
        println!("cargo:warning=failed to embed Windows resources: {err}");
    }
}
