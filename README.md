<p align="center">
  <img src="https://raw.githubusercontent.com/Moonhalf383/makedown/main/assets/icon.png" alt="Makedown 图标" width="192">
</p>

# Makedown

[English version](README.en.md)

**Makefile + Markdown = Markfile + Makedown**

Makedown 是一个把工作说明整理成实施计划的命令行工具。你在 Markfile（扩展名为 `.mf`）里写下要完成的目标、每个目标依赖的前置工作，以及完成时要核对的规格。`mkd` 读取这些内容，生成按阶段排列的 Markdown 文档。基础工作排在前面；同一阶段内的目标互不依赖，可以由实施者安排并行处理。这份文档可以交给团队成员，也可以交给协助实施的 AI 助手。

想深入了解，可继续阅读：[示例项目](examples/README.md) · [项目配置说明](docs/config.md) · [模板写法](docs/templates.md) · [Neovim 配置说明](nvim/README.md) · [VS Code 插件说明](vscode/README.md)

## 安装

通过 crates.io 安装命令行工具：

```sh
cargo install makedown-cli --locked
```

这个包会安装两个命令：`mkd` 用于检查 Markfile 和生成实施计划，`mkd-lsp` 为编辑器提供诊断、补全、跳转和格式化。安装完成后可运行 `mkd --version` 确认版本。也可以从 [GitHub Releases](https://github.com/Moonhalf383/makedown/releases) 下载适合当前平台的预编译程序。

## Markfile 长什么样？

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

项目可以由多份这样的文件组成。应用型项目入口为 `main.mf`，包型项目入口为 `lib.mf`；应用型默认把 Markfile 收进 `.mkd/`。更多由简单到复杂的完整例子见[示例项目](examples/README.md)。

## 先运行一个示例

安装 `mkd` 后，在本仓库根目录运行下面两条命令：

```sh
mkd target greet --root examples/valid/01-single-target/main.mf --check
mkd target greet --root examples/valid/01-single-target/main.mf -o greeting-plan.md
```

第一条命令检查工作说明，不写文件。第二条命令在仓库根目录生成 `greeting-plan.md`。打开它可以看到 `greet` 目标的工作说明和验收条目。如果不想在仓库里留下输出文件，把路径换成其他临时位置即可，例如 `/tmp/greeting-plan.md`（Windows 可用 `%TEMP%\greeting-plan.md`）。

这里的 `target greet` 表示从名为 `greet` 的目标开始生成计划。`--root` 指定该示例项目的入口文件；在自己的项目中，工具会从当前目录向上寻找最近的平铺或 `.mkd/` 入口文件。

## 写一份自己的工作说明

在准备存放项目的目录中运行：

```sh
mkd init my-project
cd my-project
mkd build
```

工具会在 `my-project/.mkd/` 创建 `main.mf`，在外层创建 `mkd.toml`，同时运行 `git init`，再将实施计划写入外层的 `dist/plan.md`。`mkd init --flat` 保留平铺目录方式；`mkd init --lib` 创建可打包的平铺 `lib.mf` 项目。新项目中的目标叫 `start`。完整的初始化方式和配置写法见[项目配置说明](docs/config.md)。

`mkd init` 创建的 `main.mf` 同样由上述三个区域组成，只是顶部导入区为空，文件以 `---` 开头。把示例文字改成自己的工作内容，再按同样的方式添加前置目标即可。

## 选择构建方式

`mkd target start -o plan.md` 直接为 `start` 目标生成计划，并把文件写入 `plan.md`。需要先检查而暂不写文件时，使用 `mkd target start --check`。这里的 `-o` 指定输出文件；`target` 后还可以写带模块路径的目标名，例如 `catalog::publish`，表示 `catalog` 文件模块中的 `publish` 目标。命令行里的目标路径默认从入口文件算起，不需要写 `crate::` 前缀。

若经常生成同一份计划，可以在 `mkd.toml` 里保存目标、输出路径和可选的排版模板。这组设置叫构建配方。`mkd build` 使用默认配方，`mkd build NAME` 使用指定名称的配方；`mkd target` 则直接按命令行给出的目标和输出文件运行。要检查单份文件，可用 `mkd lint .mkd/main.mf`；要格式化它，可用 `mkd fmt .mkd/main.mf`（平铺项目直接用 `main.mf`）。

## 打包与导入

以 `lib.mf` 为根的项目可以运行 `mkd pack`，得到 `dist/<项目名>.mjar`。它是保留模块结构的压缩包，公开接口来自 `lib.mf` 的公开区。将它复制到另一项目的 `.mkd/vendor/utils.mjar`，在 Markfile 顶部写 `> crate::vendor::*utils::目标 as 别名` 即可导入；`*utils` 与普通 `utils.mf` 的引用有明确区分。打包时会展开已经导入的包，没有嵌套包，也不隐藏规格原文。`mkd lock` 可记录所引用包的 SHA-256；锁内已有记录的哈希若变化，后续构建报错。用 `mkd graph --jar dist/<项目名>.mjar` 可独立查看包图。详见[包与锁说明](docs/config.md#分发-mjar-包与哈希锁)。

## 在终端查看依赖图

运行 `mkd graph` 可以把整个项目的 Target 依赖关系直接画在终端中。每份 Markfile 是一个带文件名的分组框，每个 Target 是框内的节点，箭头从前置 Target 指向依赖它的 Target。同一模块的 Target 使用相近颜色，不同模块使用不同主题色；每条边取两端颜色的中间色。颜色遵循全局 `--color auto|always|never` 设置，重定向或写入文件时默认不输出 ANSI 控制符。

```sh
mkd graph                                      # Unicode、自上而下的全项目图
mkd graph platform::launch                    # 只看该 Target 及其前置依赖
mkd graph --direction left-right              # 改为从左向右
mkd graph --charset ascii --color never       # 适合纯文本日志
mkd graph --interactive                       # 按模块浏览，选择 Target 查看详情
mkd graph --image project.svg                 # 直接导出 SVG 图片
mkd graph --format dot -o project.dot         # 导出 Graphviz DOT
mkd graph --jar dist/example.mjar             # 直接查看包图
```

交互模式以 Module 为页面、以 Target 为焦点：每页显示当前模块的全部 Target，以及它们直接依赖的外部 Target；外部节点的依赖在进入对应模块后才展开。按 `h/j/k/l` 或方向键选择相应方向的 Target，选中节点高亮，视框以焦点居中为基准；`Tab` / `Shift-Tab` 可依次选择节点。`Shift+H/J/K/L`（大写 `H/J/K/L`，也支持 Shift+方向键）按一格调整相对观察位置：H 向左、J 向下、K 向上、L 向右，图向相反方向移动。偏移在选择 Target、切换模块及返回时保留，顶部显示当前偏移；范围受视框大小限制，并尽量保持焦点可见。图边缘可以留白，让边缘 Target 也能居中。

聚焦 Target 时自动显示缩略浮窗，仅包含该 Target 的绝对逻辑路径名（例如 `module::nested::build`）和 description，支持语法高亮并动态避让焦点。缩略窗最多 48 列、10 行，长描述以 `…` 表示截断，空描述只显示路径名；它不拦截导航键，也不进入导出图片。

选中本模块 Target 后按 `Enter` 打开描述、规格和依赖的详情浮窗，缩略窗由详情窗替代；再按 `Enter` 或 `Esc` 收起详情后，缩略窗立即恢复。详情浮窗内用 `j/k` 或上下键滚动。浮窗宽度最多 72 列，按终端宽度自动换行；标题、依赖、规格、代码与强调分别高亮（遵循 `--color`）。位置优先在焦点左右或上下避让，必要时缩小；终端过小时才采用最小重叠布局。浮窗打开时也可以用 Shift 导航调整视图，浮窗随之重新布局。选中外部 Target 后，第一次 `Enter` 切换到它所属的模块并保留该 Target 的焦点，在新页面立即显示同一 Target 的缩略窗；第二次 `Enter` 才打开详情。例如 A 中的 Target 依赖 `B::four`，而 `B::four` 依赖 `C::five`：A 页只显示 `B::four`，进入 B 页后才显示 `C::five`。

浮窗关闭时，`Esc` / `Backspace` 返回上一个模块及焦点；`[` / `]` 切换上一个/下一个模块，也可访问没有依赖连线的模块。`mkd graph platform::launch --interactive` 从该 Target 所在模块开始，并选中它。按 `e` 将**当前模块局部图**（不含高亮和浮窗）导出为项目根目录下的 `graph.svg`，按 `Ctrl-C` 退出。可用 `mkd graph --interactive --image dist/project.svg` 为 `e` 指定其他导出位置。

`--image [FILE]` 使用内置导出器直接生成带主题色的矢量 SVG，不要求安装 Graphviz；省略 `FILE` 时写入配置目录下的 `graph.svg`（`--jar` 则写在包旁），相对路径也以该目录为起点。`--direction` 还接受 `bottom-up` 和 `right-left`。DOT 文件仍可继续用 Graphviz 转换成其他格式，例如 `dot -Tpng project.dot -o project.png`。当前图命令只渲染分析成功的项目；若存在语法、引用或依赖环错误，会像其他项目命令一样输出诊断并返回失败。

完成一轮工作后可运行 `mkd archive`。平铺布局暂存整个项目目录；隐藏布局只暂存 `.mkd/`、`mkd.toml` 和已有的 `mkd.lock`，不会顺带提交宿主源码。提交消息由概括数量的主题行和按动作分组的 Target 清单组成，例如：

```text
Archive targets: add 2, update 1, remove 1

Add:
- release
- docs

Update:
- test

Remove:
- draft
```

若只有普通项目文件变化，则使用单行消息 `Archive project changes`。首次提交前需按 Git 的常规方式配置 `user.name` 和 `user.email`。有关命令和路径的具体规则见[项目配置说明](docs/config.md)。

生成的计划有内置 Markdown 排版。想改成自己的任务清单或交接手册，可以阅读[模板写法](docs/templates.md)和[五种中文模板示例](examples/templates/README.md)。使用编辑器时，可按 [Neovim 配置说明](nvim/README.md)或 [VS Code 插件说明](vscode/README.md)启用 `.mf` 和模板文件的高亮、诊断与补全。
