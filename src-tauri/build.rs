use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    generate_embedded_patch_engine();
    tauri_build::build()
}

fn generate_embedded_patch_engine() {
    println!("cargo:rerun-if-env-changed=PATCH_ENGINE_SOURCE_DIR");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let out_file = out_dir.join("embedded_patch_engine.rs");
    let source = patch_engine_source_dir();

    if !source.join("scripts").join("install_windows.ps1").exists() {
        println!(
            "cargo:warning=patch engine source not found at {}; building without embedded patch engine",
            source.display()
        );
        fs::write(
            out_file,
            "const EMBEDDED_PATCH_ENGINE: &[EmbeddedPatchFile] = &[];\n",
        )
        .expect("write empty embedded patch engine manifest");
        return;
    }

    println!("cargo:rerun-if-changed={}", source.display());

    let mut files = Vec::new();
    collect_patch_engine_files(&source, &source, &mut files);
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut manifest = String::from("const EMBEDDED_PATCH_ENGINE: &[EmbeddedPatchFile] = &[\n");
    for (relative_path, absolute_path) in files {
        let include_path = absolute_path.to_string_lossy().replace('\\', "/");
        manifest.push_str("    EmbeddedPatchFile {\n");
        manifest.push_str(&format!("        relative_path: {:?},\n", relative_path));
        manifest.push_str(&format!(
            "        bytes: include_bytes!({:?}),\n",
            include_path
        ));
        manifest.push_str("    },\n");
    }
    manifest.push_str("];\n");

    fs::write(out_file, manifest).expect("write embedded patch engine manifest");
}

fn patch_engine_source_dir() -> PathBuf {
    if let Some(path) = env::var_os("PATCH_ENGINE_SOURCE_DIR") {
        return PathBuf::from(path);
    }

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
    let vendor = manifest_dir
        .join("..")
        .join("vendor")
        .join("claude-desktop-zh-cn");
    if vendor.join("scripts").join("install_windows.ps1").exists() {
        return vendor;
    }

    manifest_dir.join("..").join("..").join("claude-desktop-zh-cn")
}

fn collect_patch_engine_files(root: &Path, current: &Path, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if should_skip_patch_engine_entry(&file_name) {
            continue;
        }

        if path.is_dir() {
            collect_patch_engine_files(root, &path, files);
            continue;
        }

        if !path.is_file() {
            continue;
        }

        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        files.push((relative, path));
    }
}

fn should_skip_patch_engine_entry(name: &str) -> bool {
    matches!(name, ".git" | ".github" | "docs") || name.ends_with(".log")
}
