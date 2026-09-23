# 在 Neovim 中编辑 Markfile

支持 Neovim 0.11 及以上版本。高亮由独立的 Vim syntax 文件提供，无需 LSP；`mkd-lsp` 额外提供未保存内容的诊断和全文格式化。

```sh
cargo build --bin mkd-lsp
```

在 LazyVim 的 `~/.config/nvim/init.lua` 中，**必须先初始化 LazyVim，再加载本项目配置**（将路径改为本地绝对路径）：

```lua
require("config.lazy")
dofile("/path/to/makedown/nvim/lsp.lua")
```

LazyVim 初始化时会重建 `runtimepath`。如果把 `dofile` 放在 `require("config.lazy")` **之前**，本项目的语法文件可能不再位于运行路径上，Neovim 就会加载系统自带的 Metafont `syntax/mf.vim`，虽然文件类型仍显示 `mf`，却没有 Markfile 高亮。不要仅通过 `:set filetype?` 判断是否配置成功。

`nvim/lsp.lua` 会把本目录加入 `runtimepath`、识别 `.mf` 文件，并以 `target/debug/mkd-lsp` 启动语言服务器。独立使用高亮时，只需将仓库的 `nvim` 目录加入 Neovim 的 `runtimepath` 并开启 `filetype` 和 `syntax`。打开 `.mf` 后可检查：

```vim
:set filetype?
:lua print(vim.inspect(vim.api.nvim_get_runtime_file("syntax/mf.vim", true)))
:lua print(vim.fn.synIDattr(vim.fn.synID(3, 1, 1), "name"))
```

第二项结果的首个路径应当是本项目的 `nvim/syntax/mf.vim`；第三项在目标标题 `# ...` 所在行应显示 `mfTargetMarker`。

使用 `vim.lsp.buf.format()`（例如手动执行 `:lua vim.lsp.buf.format()`）对当前缓冲区请求格式化。服务器不会自己写磁盘，编辑器确认并应用修改后才会保存。

## 范围与后续架构

- `src/bin/mkd-lsp.rs`：独立进程入口，通过 stdin/stdout 使用 LSP JSON-RPC；stdout 不输出普通日志。
- `src/lsp.rs`：维护打开文档的内存全文和版本；对 `didOpen`、`didChange` 发布语法与质量诊断，对 `didClose` 清空诊断；响应 `textDocument/formatting`。采用全文同步；诊断范围按 UTF-16 编码。
- `src/parser.rs`、`src/linter.rs`、`src/formatter.rs`：与 CLI 共享单文件语言规则；编辑器内容不会从磁盘读取。
- `nvim/ftdetect/mf.vim`、`nvim/syntax/mf.vim`：识别文件类型并高亮分隔线、目标、导入/依赖、锚点、别名和规格。

此阶段**不提供跨文件语义诊断、自动补全或跳转定义**。实现这些能力时需要给项目分析器提供内存优先的源码读取接口、可维护的项目索引，以及更精确的语法节点位置。高亮采用轻量正则，可能把描述中行首的结构标记当作高亮标记；诊断仍以解析器为准。
