# Makedown for VS Code

在 VS Code 中编辑 Markfile 工作说明（`.mf`）和 Markdown 排版模板（`.md.j2`）。Markfile 中的目标（Target）是一项工作，可以写下前置目标和验收规格；`mkd` 会把这些工作整理为分阶段的 Markdown 实施计划。关于 Markfile 的写法与命令行工具，见[项目 README](https://github.com/Moonhalf383/makedown#readme)。

本插件提供语法高亮，并内置各平台版本的语言服务器 `mkd-lsp`（负责即时报错、补全等编辑辅助）：打开或编辑文件时即时检查，输入时给出候选；`.mf` 文件还支持跳转定义、查找引用和格式化。插件开箱即用，不需要用户安装 Rust 或任何其他组件。微软官方 VS Code 用户可从 [Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=Moonhalf383.makedown) 安装；Code - OSS、VSCodium 等使用 Open VSX 的编辑器可从 [Open VSX Registry](https://open-vsx.org/extension/Moonhalf383/makedown) 安装。

## 功能一览

在 `.mf` 文件中：

- 语法高亮：目标标题、`>` 导入与前置目标、`-` 验收规格和 `---` 分隔线各有配色。
- 诊断：未保存的修改也会即时重新检查，错误以波浪线标出，并显示在“问题”面板。
- 补全：输入 `>`、`:` 或 `.` 后自动给出候选，包括 `crate::`、`self::`、`super::` 路径和公开目标。
- 跳转与引用：在目标名称、导入路径或前置目标上转到定义（F12），查找所有引用（Shift+F12）。
- 格式化：右键菜单“使用...格式化文档”，或按 Shift+Alt+F。

在 `.md.j2` 模板中，Markdown 高亮会叠加 MiniJinja 语法（类似 Jinja2 的 `{{ 占位符 }}` 模板写法）；语言服务检查模板语法错误（`T001`）和未知 `plan` 字段（`T002`），并在 `plan.`、`stage.`、`target.` 之后补全字段。

插件只负责编辑。检查或生成实施计划、用真实数据渲染模板，仍在终端运行 `mkd`，例如 `mkd target release -o plan.md` 或 `mkd target release --check --template plan.md.j2`。

## 本地构建

构建过程需要 Rust、Node.js 22 或更新版本、npm 以及当前平台的构建工具。在仓库根目录依次运行：

```sh
make vscode-deps
make vscode-test
make vscode-package
```

最后一个命令会构建 Rust 语言服务器，把可执行文件和仓库根目录的 MIT 许可证复制进插件目录，再调用 `vsce` 打包；不要绕过它直接运行 `vsce package`。生成的 `vscode/*.vsix` 可通过 VS Code 扩展视图的“从 VSIX 安装”进行安装。

## 支持的平台

VSIX 为六种桌面平台分别打包：Windows、macOS、Linux 的 x64 与 ARM64。浏览器版 VS Code、32 位系统和 Alpine Linux 尚未支持。远程开发时，插件会安装到远程（工作区所在机器）一端，需要为那台机器选择匹配平台的 VSIX。问题反馈请使用 [GitHub Issues](https://github.com/Moonhalf383/makedown/issues)。
