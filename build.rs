use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/convertalot.ico");
    if env::var("CARGO_CFG_TARGET_OS").expect("target os") != "windows" {
        return;
    }

    // GNU windres splits unquoted include/input paths on spaces. This workspace
    // lives under "claude code", so compile from a space-free staging dir using
    // relative names only.
    let work = env::temp_dir().join("convertalot-winresource");
    fs::create_dir_all(&work).expect("temp resource dir");
    fs::copy("assets/convertalot.ico", work.join("convertalot.ico")).expect("copy icon");
    fs::write(work.join("resource.rc"), resource_script()).expect("write resource.rc");

    match env::var("CARGO_CFG_TARGET_ENV")
        .expect("target env")
        .as_str()
    {
        "gnu" => compile_gnu(&work),
        "msvc" => compile_msvc(&work),
        other => panic!("unsupported Windows target env {other}"),
    }
}

fn resource_script() -> String {
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut parts = version.split('.');
    let major = parts.next().unwrap_or("0");
    let minor = parts.next().unwrap_or("0");
    let patch = parts
        .next()
        .unwrap_or("0")
        .split(['-', '+'])
        .next()
        .unwrap_or("0");

    format!(
        r#"#pragma code_page(65001)
1 VERSIONINFO
FILEVERSION {major}, {minor}, {patch}, 0
PRODUCTVERSION {major}, {minor}, {patch}, 0
FILEOS 0x40004
FILETYPE 0x1
FILESUBTYPE 0x0
FILEFLAGSMASK 0x3f
FILEFLAGS 0x0
{{
BLOCK "StringFileInfo"
{{
BLOCK "000004b0"
{{
VALUE "FileDescription", "Convertalot image converter"
VALUE "FileVersion", "{version}"
VALUE "ProductName", "Convertalot"
VALUE "ProductVersion", "{version}"
}}
}}
BLOCK "VarFileInfo" {{
VALUE "Translation", 0x0, 0x04b0
}}
}}
1 ICON "convertalot.ico"
"#
    )
}

fn compile_gnu(work: &Path) {
    let windres = env::var("WINDRES").unwrap_or_else(|_| "windres".into());
    let ar = env::var("AR").unwrap_or_else(|_| "ar".into());
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let mut windres_cmd = Command::new(&windres);
    windres_cmd.current_dir(work);
    match arch.as_str() {
        "x86_64" => {
            windres_cmd.args(["--target", "pe-x86-64"]);
        }
        "x86" => {
            windres_cmd.args(["--target", "pe-i386"]);
        }
        _ => {}
    }
    run(
        windres_cmd.arg("resource.rc").arg("resource.o"),
        "windres",
    );
    run(
        Command::new(&ar)
            .current_dir(work)
            .args(["rsc", "libresource.a", "resource.o"]),
        "ar",
    );
    println!("cargo:rustc-link-search=native={}", work.display());
    println!("cargo:rustc-link-lib=static:+whole-archive=resource");
}

fn compile_msvc(work: &Path) {
    // GitHub's windows-latest image (and VS Developer Prompt-less shells) have
    // the Windows SDK installed, but rc.exe is not on PATH.
    let rc = find_rc_exe();
    run(
        Command::new(&rc)
            .current_dir(work)
            .args(["/nologo", "/fo", "resource.res", "resource.rc"]),
        "rc.exe",
    );
    let res = work.join("resource.res");
    println!("cargo:rustc-link-arg={}", res.display());
}

fn find_rc_exe() -> PathBuf {
    if let Some(path) = env::var_os("RC_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return path;
        }
        panic!("RC_PATH does not point to rc.exe: {}", path.display());
    }
    if let Some(path) = find_on_path("rc.exe") {
        return path;
    }
    if let Some(path) = find_windows_kit_rc() {
        return path;
    }
    panic!(
        "rc.exe not found. Install the Windows SDK, or set RC_PATH to the resource compiler."
    );
}

fn find_on_path(exe: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(exe);
        candidate.is_file().then_some(candidate)
    })
}

fn find_windows_kit_rc() -> Option<PathBuf> {
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default().as_str() {
        "x86_64" => "x64",
        "x86" => "x86",
        "aarch64" => "arm64",
        other => panic!("unsupported Windows arch {other} for rc.exe lookup"),
    };
    let mut found = Vec::new();
    for root in [
        PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin"),
        PathBuf::from(r"C:\Program Files\Windows Kits\10\bin"),
    ] {
        let direct = root.join(arch).join("rc.exe");
        if direct.is_file() {
            found.push(direct);
        }
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let version_dir = entry.path();
            if !version_dir.is_dir() {
                continue;
            }
            for candidate in [
                version_dir.join(arch).join("rc.exe"),
                version_dir.join("Hostx64").join(arch).join("rc.exe"),
                version_dir.join("Hostx86").join(arch).join("rc.exe"),
            ] {
                if candidate.is_file() {
                    found.push(candidate);
                }
            }
        }
    }
    found.sort();
    found.pop()
}

fn run(command: &mut Command, name: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("failed to start {name}: {error}"));
    if !status.success() {
        panic!("{name} failed with {status}");
    }
}
