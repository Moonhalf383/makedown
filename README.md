<p align="center">
  <img src="https://raw.githubusercontent.com/Moonhalf383/makedown/main/assets/icon.png" alt="Makedown 图标" width="192">
</p>

```text
███╗   ███╗ █████╗ ██╗  ██╗███████╗██████╗  ██████╗ ██╗    ██╗███╗   ██╗
████╗ ████║██╔══██╗██║ ██╔╝██╔════╝██╔══██╗██╔═══██╗██║    ██║████╗  ██║
██╔████╔██║███████║█████╔╝ █████╗  ██║  ██║██║   ██║██║ █╗ ██║██╔██╗ ██║
██║╚██╔╝██║██╔══██║██╔═██╗ ██╔══╝  ██║  ██║██║   ██║██║███╗██║██║╚██╗██║
██║ ╚═╝ ██║██║  ██║██║  ██╗███████╗██████╔╝╚██████╔╝╚███╔███╔╝██║ ╚████║
╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝╚═════╝  ╚═════╝  ╚══╝╚══╝ ╚═╝  ╚═══╝

   ██╗
   ██║
████████╗
██╔═██╔═╝
██████║
╚═════╝

███╗   ███╗ █████╗ ██████╗ ██╗  ██╗███████╗██╗██╗     ███████╗
████╗ ████║██╔══██╗██╔══██╗██║ ██╔╝██╔════╝██║██║     ██╔════╝
██╔████╔██║███████║██████╔╝█████╔╝ █████╗  ██║██║     █████╗
██║╚██╔╝██║██╔══██║██╔══██╗██╔═██╗ ██╔══╝  ██║██║     ██╔══╝
██║ ╚═╝ ██║██║  ██║██║  ██║██║  ██╗██║     ██║███████╗███████╗
╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝     ╚═╝╚══════╝╚══════╝
```

---

## Makefile + Markdown == Markfile + Makedown

Makedown 是一个把工作说明整理成实施计划的命令行工具。你在 Markfile（扩展名为 `.mf`）里写下要完成的目标、每个目标依赖的前置工作，以及完成时要核对的规格。`mkd` 读取这些内容，生成按阶段排列的 Markdown 文档。基础工作排在前面；同一阶段内的目标互不依赖，可以由实施者安排并行处理。这份文档可以交给团队成员，也可以交给协助实施的 AI 助手。

想深入了解，可继续阅读：[示例项目](examples/README.md) · [项目配置说明](docs/config.md) · [模板写法](docs/templates.md) · [Neovim 配置说明](nvim/README.md) · [VS Code 插件说明](vscode/README.md)

### Markfile 长什么样？

下面是一份真实的 Markfile 片段，描述“组装并批准完整发布包”这项工作。文件分为三个区域：顶部用 `>` 引入其他文件中的目标，中间用 `---` 分隔出目标主体，底部再以 `> release` 把这个目标公开给其他文件使用。

```makedown
> super::build::backend::package_backend
> super::build::frontend::package_frontend
> super::quality::gate::approve
---
# release
> package_backend
> package_frontend
> approve
组装并批准完整发布包。
- 前后端制品使用相同契约版本。
- 质量门禁结果随发布包归档。
---
> release
```

目标主体从 `# release` 开始。标题下方可以写一段普通文字，说明这项工作要做什么；以 `-` 开头的行是验收规格，写明完成时可以逐条核对的结果。标题和描述之间还可以用 `>` 列出前置目标——`package_backend`、`package_frontend` 和 `approve`。编译时，`mkd` 先安排这些前置工作，再安排 `release`，从而形成分阶段的实施计划。

顶部三行的 `super::` 是一种文件路径写法：`super` 指向当前文件的上一级目录，连续的 `::` 表示继续进入下级文件。例如 `super::quality::gate::approve` 表示从上一级目录的 `quality/gate.mf` 文件导入名为 `approve` 的目标，并把它以自己的名字 `approve` 引入本文件——正文的前置目标因此可以直接写短名。`>` 出现在文件顶部是导入，出现在标题下方则是前置目标。类似的前缀还有 `crate::`（从项目入口文件出发）和 `self::`（从当前文件所在目录出发）。被导入的目标必须先在自己的文件中用底部的 `>` 声明公开，否则会报告私有目标错误。

项目可以由多份这样的文件组成，入口文件默认为 `main.mf`。更多由简单到复杂的完整例子见[示例项目](examples/README.md)。

## 先运行一个示例

项目用 Rust 编写。从仓库根目录运行下面两条命令即可体验，无须提前安装 `mkd`：

```sh
cargo run --quiet --bin mkd -- target greet --root examples/valid/01-single-target/main.mf --check
cargo run --quiet --bin mkd -- target greet --root examples/valid/01-single-target/main.mf -o greeting-plan.md
```

第一条命令检查工作说明，不写文件。第二条命令在仓库根目录生成 `greeting-plan.md`。打开它可以看到 `greet` 目标的工作说明和验收条目。如果不想在仓库里留下输出文件，把路径换成其他临时位置即可，例如 `/tmp/greeting-plan.md`（Windows 可用 `%TEMP%\greeting-plan.md`）。

这里的 `target greet` 表示从名为 `greet` 的目标开始生成计划。`--root` 指定该示例项目的入口文件；在自己的项目中，工具会从当前目录向上寻找最近的 `main.mf`。从源码运行时，`cargo run --quiet --bin mkd --` 后面的内容就是传给 `mkd` 的参数；已经有可执行程序时，可以直接写 `mkd target greet ...`。

## 安装与写一份自己的工作说明

通过 crates.io 安装命令行工具：

```sh
cargo install makedown-cli --locked
```

安装完成后，在准备存放项目的目录中运行：

```sh
mkd init my-project
cd my-project
mkd build
```

工具会在 `my-project` 中创建入口文件 `main.mf` 和构建设置 `mkd.toml`，再将实施计划写入 `dist/plan.md`。新项目中的目标叫 `start`。也可以从 [GitHub Releases](https://github.com/Moonhalf383/makedown/releases) 下载适合当前平台的预编译程序。完整的初始化方式和配置写法见[项目配置说明](docs/config.md)。

`mkd init` 创建的 `main.mf` 同样由上述三个区域组成，只是顶部导入区为空，文件以 `---` 开头。把示例文字改成自己的工作内容，再按同样的方式添加前置目标即可。

## 选择构建方式

`mkd target start -o plan.md` 直接为 `start` 目标生成计划，并把文件写入 `plan.md`。需要先检查而暂不写文件时，使用 `mkd target start --check`。这里的 `-o` 指定输出文件；`target` 后还可以写带模块路径的目标名，例如 `catalog::publish`，表示 `catalog` 文件模块中的 `publish` 目标。命令行里的目标路径默认从入口文件算起，不需要写 `crate::` 前缀。

若经常生成同一份计划，可以在 `mkd.toml` 里保存目标、输出路径和可选的排版模板。这组设置叫构建配方。`mkd build` 使用默认配方，`mkd build NAME` 使用指定名称的配方；`mkd target` 则直接按命令行给出的目标和输出文件运行。要检查单份文件，可用 `mkd lint main.mf`；要格式化它，可用 `mkd fmt main.mf`。有关命令和路径的具体规则见[项目配置说明](docs/config.md)。

生成的计划有内置 Markdown 排版。想改成自己的任务清单或交接手册，可以阅读[模板写法](docs/templates.md)和[五种中文模板示例](examples/templates/README.md)。使用编辑器时，可按 [Neovim 配置说明](nvim/README.md)或 [VS Code 插件说明](vscode/README.md)启用 `.mf` 和模板文件的高亮、诊断与补全。
