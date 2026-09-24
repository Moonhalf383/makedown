# 在 Neovim 中编辑 Markfile

Markfile 是扩展名为 `.mf` 的工作说明文件。一个目标（Target）描述一项工作，包括说明、前置工作和验收条目；`mkd` 可以把这些内容整理成 Markdown 实施计划。如果你用 Neovim 编辑项目，本目录提供文件高亮和语言服务器支持。高亮让文件结构更容易看清；语言服务器 `mkd-lsp` 可以在编辑时报告错误，并提供补全、跳转和格式化。这里的配置适用于 Neovim 0.11 及以上版本。

## 安装与启用

先在仓库根目录构建语言服务器：

```sh
cargo build --bin mkd-lsp
```

这会生成 `target/debug/mkd-lsp`。如果使用 LazyVim，在 `~/.config/nvim/init.lua` 中先加载 LazyVim，再加载本项目的配置文件。将下面的仓库路径换成自己电脑上的绝对路径：

```lua
require("config.lazy")
dofile("/path/to/makedown/nvim/lsp.lua")
```

`nvim/lsp.lua` 会让 Neovim 找到本项目的语法文件，识别 `.mf` 和 `.md.j2` 文件，并启动刚构建的 `mkd-lsp`。要把 `dofile(...)` 放在 `require("config.lazy")` 后面，否则 LazyVim 初始化可能会覆盖本项目加入的搜索路径。Neovim 把这个路径称为 `runtimepath`。如果语言服务器没有启动，请先确认构建命令已完成，并检查 `target/debug/mkd-lsp` 是否存在。

只想使用高亮时，无须构建或启动语言服务器。将仓库的 `nvim` 目录加入 Neovim 的 `runtimepath`，并开启文件类型检测和语法高亮即可，例如在 Neovim 中执行 `:filetype on` 和 `:syntax on`。使用 LazyVim 的上述配置则会同时启用高亮与语言服务器。

## 检查 .mf 文件高亮

在仓库根目录启动 Neovim，打开 `examples/valid/01-single-target/main.mf`，确认已运行 `:syntax on`。这个文件第 2 行是目标标题 `# greet`。然后运行以下命令查看文件类型、实际加载的语法文件以及该标题行首的高亮名称：

```vim
:set filetype?
:lua print(vim.inspect(vim.api.nvim_get_runtime_file("syntax/mf.vim", true)))
:lua print(vim.fn.synIDattr(vim.fn.synID(2, 1, 1), "name"))
```

第二条命令列出的第一个路径应是本项目的 `nvim/syntax/mf.vim`。第三条命令中的 `2, 1` 表示第 2 行第 1 列，结果应是 `mfTargetMarker`。检查其他文件时，把 `2` 改成目标标题 `# ...` 所在的行号。检查实际路径很重要，因为 Neovim 也自带一个用于 Metafont 的 `syntax/mf.vim`：即使 `:set filetype?` 显示 `mf`，也可能加载了另一份语法文件。

## 编辑目标时可用的操作

语言服务器会检查打开和正在修改的 `.mf` 文件，包括尚未保存的内容。导入其他文件的目标、填写前置目标，以及声明可供其他文件引用的公开目标时，可以使用补全。把光标放在目标名称、导入路径或前置目标上，运行 `:lua vim.lsp.buf.definition()` 可以跳到定义；运行 `:lua vim.lsp.buf.references()` 可以查看引用位置。

本项目的配置会自动启用补全。输入 `>`、`:` 或空格可以触发候选，也可以依次按 Ctrl-X、Ctrl-O（Neovim 记为 `<C-x><C-o>`）手动请求。在 `>` 后补一个空格，有助于让 Neovim 再次显示候选。候选出现后，可按 `<C-n>` 或 `<C-p>` 选择；预览窗口会显示目标的说明、前置工作和规格，或者列出模块中公开的目标。补全还支持文件路径前缀：`crate::` 从项目入口开始，`self::` 从当前文件所在目录开始，`super::` 从上一级目录开始。导入候选依次显示这些路径前缀、公开目标和模块路径；前置工作候选显示本文件目标和已导入的别名。例如 `> crate::shared::lint as shared_lint` 中，`as` 为导入的目标起一个本文件内使用的名字；补全目前不会建议这个名字。

候选菜单采用 `menuone,noselect,popup` 设置，打开时不会自行选中或插入第一项。某些 Neovim 配置需要在选择候选后额外创建文档预览窗口，本项目的 `lsp.lua` 会处理这一点。如果安装了可选界面插件 Noice 并启用了它的外部补全菜单，预览会放在菜单旁边，避免被遮挡。

要格式化当前 `.mf` 文件，运行：

```vim
:lua vim.lsp.buf.format()
```

语言服务器把格式化结果交给编辑器，不直接写入磁盘；检查编辑器中的结果后再保存文件即可。

## 编辑 Markdown 模板

扩展名为 `.md.j2` 的文件是实施计划的 Markdown 排版模板，可以包含 `{{ ... }}` 等 MiniJinja 占位符。打开这类文件时，本项目会用 `mkd_template` 文件类型，在普通 Markdown 高亮上叠加模板语法高亮。普通 `.j2` 文件仍由其他配置处理。模板的基本用法和字段见[模板说明](../docs/templates.md)。

可以打开 `examples/templates/02-写作任务清单.md.j2`，再运行以下命令确认高亮：

```vim
:set filetype?
:lua print(vim.fn.synIDattr(vim.fn.synID(2, 1, 1), "name"))
:lua print(vim.fn.synIDattr(vim.fn.synID(5, 1, 1), "name"))
```

文件类型应显示 `mkd_template`，两条高亮检查命令应分别显示 `markdownH1Delimiter` 和 `mkdTemplateStatement`。语言服务器会诊断模板语法错误（编号 `T001`）和明确写出的未知 `plan` 字段（编号 `T002`）；在模板标签内输入 `plan.`、`stage.` 或 `target.` 的点号后，还会列出可用字段。这里的 `plan` 是整个实施计划，`stage` 是当前阶段，`target` 是当前目标。也可以按 `<C-x><C-o>` 手动补全。

要确认模板能否处理某个项目的实际数据，可在终端运行：

```sh
mkd target start --check --template plan.md.j2
```

请把 `start` 换成你的目标名称，把 `plan.md.j2` 换成模板路径。这个命令会尝试生成内容，但不会写文件。编辑器只检查模板的部分静态写法，不会用项目中的真实目标数据来执行模板；模板文件的格式化、跳转定义和引用查找目前也未提供。

## 功能范围与实现位置

查看诊断时，可以先打开报错的 `.mf` 文件。语言服务器会分析这份文件导入到的其他文件，并在导入位置提示找不到的模块；如果被导入的文件自身有错误，打开那份文件可查看对应诊断。`mkd lint --all` 适合需要一次检查项目里全部 `.mf` 文件的场景，包括尚未被导入的文件。补全和引用查找则使用整个项目的文件索引，也会考虑编辑器里尚未保存的内容。

打开文件改变内容或关闭时，索引会在下一次请求时重建；如果文件被编辑器之外的程序直接改动，目前不会立刻刷新索引。大型项目的增量索引和重命名功能尚未提供。高亮依据行首符号匹配，偶尔可能把说明中的字符当成结构标记；诊断以实际解析的结果为准。

如需查阅实现，可从以下文件入手：`src/bin/mkd-lsp.rs` 启动语言服务器；`src/lsp.rs` 处理编辑器请求和未保存内容；`src/lsp/workspace.rs` 管理跨文件查找。服务器与命令行程序共享 `src/parser.rs`、`src/linter.rs`、`src/formatter.rs` 和 `src/project.rs` 的规则。`nvim/ftdetect/` 识别文件类型，`nvim/syntax/` 提供高亮规则。
