fn main() {
    // 前端 dist 变化时强制重编译，确保 generate_context! 重新嵌入最新资产
    println!("cargo:rerun-if-changed=../dist");
    tauri_build::build()
}
