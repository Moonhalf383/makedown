// 启动与编辑器通信的 Markfile 语言服务器。
fn main() -> Result<(), Box<dyn std::error::Error>> {
    makedown::lsp::serve()
}
