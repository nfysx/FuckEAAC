// build.rs —— Tauri 构建脚本（编译前生成上下文：图标、权限、资源等）
// 这是 tauri-build 的标准用法，一般不需要改。
fn main() {
    tauri_build::build()
}
