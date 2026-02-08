fn main() {
    println!("cargo:rerun-if-changed=web/src");
    println!("cargo:rerun-if-changed=web/index.html");
    println!("cargo:rerun-if-changed=web/vite.config.ts");
    println!("cargo:rerun-if-changed=web/package.json");

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let dist_dir = out_dir.join("web-dist");

    // Only build web assets if the web feature is enabled
    if std::env::var("CARGO_FEATURE_WEB").is_ok() {
        build_web(&out_dir, &dist_dir);
    }

    // Tell rust-embed where to find the built assets
    println!("cargo:rustc-env=WEB_DIST_DIR={}", dist_dir.display());
}

fn build_web(out_dir: &std::path::Path, dist_dir: &std::path::Path) {
    let web_src = std::path::Path::new("web");
    // Copy web source into OUT_DIR so npm install/build never touches the source tree.
    // This is required for `cargo publish --verify` which rejects source modifications.
    let web_build = out_dir.join("web-build");
    if web_build.exists() {
        std::fs::remove_dir_all(&web_build).ok();
    }
    copy_dir(web_src, &web_build);

    // Install dependencies
    if !web_build.join("node_modules").exists() {
        let status = std::process::Command::new("npm")
            .args(["install"])
            .current_dir(&web_build)
            .status();
        if status.is_err() || !status.unwrap().success() {
            println!("cargo:warning=npm install failed; web UI will not be available");
            write_placeholder(dist_dir, "Web UI not built. Install npm and rebuild.");
            return;
        }
    }

    // Build with output directed to dist_dir
    let status = std::process::Command::new("npm")
        .args([
            "run",
            "build",
            "--",
            "--outDir",
            &dist_dir.to_string_lossy(),
        ])
        .current_dir(&web_build)
        .status();
    if status.is_err() || !status.unwrap().success() {
        println!("cargo:warning=npm run build failed; web UI will not be available");
        write_placeholder(dist_dir, "Web UI build failed.");
    }
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).ok();
    if let Ok(entries) = std::fs::read_dir(src) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            // Skip node_modules and dist — they're build artifacts
            if name == "node_modules" || name == "dist" {
                continue;
            }
            let target = dst.join(&name);
            if path.is_dir() {
                copy_dir(&path, &target);
            } else {
                std::fs::copy(&path, &target).ok();
            }
        }
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
