fn main() {
    println!("cargo:rerun-if-changed=web/src");
    println!("cargo:rerun-if-changed=web/index.html");
    println!("cargo:rerun-if-changed=web/vite.config.ts");
    println!("cargo:rerun-if-changed=web/package.json");

    // Only build web assets if the web feature is enabled
    if std::env::var("CARGO_FEATURE_WEB").is_ok() {
        build_web();
    }
}

fn build_web() {
    let web_dir = std::path::Path::new("web");
    let dist_dir = web_dir.join("dist");

    // Check if node_modules exists, if not run npm install
    if !web_dir.join("node_modules").exists() {
        let status = std::process::Command::new("npm")
            .args(["install"])
            .current_dir(web_dir)
            .status();
        if status.is_err() || !status.unwrap().success() {
            println!("cargo:warning=npm install failed; web UI will not be available");
            write_placeholder(&dist_dir, "Web UI not built. Install npm and rebuild.");
            return;
        }
    }

    // Run npm build
    let status = std::process::Command::new("npm")
        .args(["run", "build"])
        .current_dir(web_dir)
        .status();
    if status.is_err() || !status.unwrap().success() {
        println!("cargo:warning=npm run build failed; web UI will not be available");
        write_placeholder(&dist_dir, "Web UI build failed.");
    }
}

fn write_placeholder(dist_dir: &std::path::Path, message: &str) {
    std::fs::create_dir_all(dist_dir).ok();
    std::fs::write(
        dist_dir.join("index.html"),
        format!("<html><body>{message}</body></html>"),
    )
    .ok();
}
