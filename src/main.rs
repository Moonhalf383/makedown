// 启动 mkd 命令行并返回对应进程状态。
fn main() -> std::process::ExitCode {
    makedown::cli::run()
}
