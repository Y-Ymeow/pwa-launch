use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    tauri_build::build();

    // 复制自定义 AppRun 脚本到目标目录（用于 AppImage）
    let out_dir = env::var("OUT_DIR").unwrap();
    let apprun_src = PathBuf::from("AppRun");
    let apprun_dst = PathBuf::from(&out_dir).join("../../../AppRun");
    
    if apprun_src.exists() {
        fs::copy(&apprun_src, &apprun_dst).expect("Failed to copy AppRun");
        println!("cargo:rerun-if-changed=AppRun");
    }
}
