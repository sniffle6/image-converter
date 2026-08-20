use std::env;
use std::fs;
use std::path::Path;
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
    let rc = env::var("RC_PATH").unwrap_or_else(|_| "rc.exe".into());
    run(
        Command::new(&rc)
            .current_dir(work)
            .args(["/nologo", "/fo", "resource.res", "resource.rc"]),
        "rc.exe",
    );
    let res = work.join("resource.res");
    println!("cargo:rustc-link-arg={}", res.display());
}

fn run(command: &mut Command, name: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("failed to start {name}: {error}"));
    if !status.success() {
        panic!("{name} failed with {status}");
    }
}
