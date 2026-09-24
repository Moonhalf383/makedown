/// 提供 mkd 命令行交互能力。
pub mod cli;
/// 提供规格制品编译能力。
pub mod compiler;
/// 提供项目构建配方的加载与校验。
pub mod config;
/// 定义已解析项目的核心领域模型。
pub mod core;
/// 提供保留规格内容的 Markfile 格式化能力。
pub mod formatter;
/// 提供文件与项目静态检查能力。
pub mod linter;
/// 提供可复用的 Cargo 风格日志接口。
pub mod logging;
/// 提供编辑器中的单文件诊断与格式化服务。
pub mod lsp;
/// 提供单个 Markfile 的语法解析能力。
pub mod parser;
/// 提供项目加载、引用解析与图校验能力。
pub mod project;
