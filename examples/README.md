# 从示例认识 Markfile 项目

Markfile 是以 `.mf` 为扩展名的工作说明文件。一个文件可以描述几个目标（Target）：每个目标是一项工作，可以包含说明、前置工作和完成时要核对的规格。`mkd` 根据这些信息生成分阶段的 Markdown 实施计划。

建议先打开 [`valid/01-single-target/main.mf`](valid/01-single-target/main.mf)。这个文件中有一个名为 `greet` 的目标：`# greet` 开始描述目标，下一行是工作说明，`-` 开头的行写验收条目。文件末尾的 `> greet` 把这个目标设为可供其他文件引用的公开目标。在仓库根目录运行以下命令，就能检查该示例并生成计划：

```sh
cargo run --quiet --bin mkd -- target greet --root examples/valid/01-single-target/main.mf --check
cargo run --quiet --bin mkd -- target greet --root examples/valid/01-single-target/main.mf -o /tmp/greet-plan.md
```

这里的 `cargo run --quiet --bin mkd --` 是从源码运行程序的方式。若已经安装 `mkd`，可将这一段替换为 `mkd`。`--root` 指定项目的入口文件；`--check` 只检查而不写文件，`-o` 指定输出文件。更多构建命令和配置文件写法见[项目配置说明](../docs/config.md)。

`valid/` 中的示例逐步增加了前置目标、跨文件引用、嵌套模块和并行阶段。这里的“模块”可以理解为用另一份 `.mf` 文件保存的一组目标。例如，`catalog::publish` 表示 `catalog` 模块中的 `publish` 目标。想看模板如何把计划排成不同文档，可继续阅读[中文模板示例](templates/README.md)。

`invalid/` 收集常见的输入错误，例如目标重名、引用不存在的模块以及依赖形成循环。每个示例目录中的 `expected.txt` 是自动化测试用的简短说明，第一行记录要从哪个目标开始。在 `valid/` 中，它还记录预期的阶段数，例如 `stages=2`；在 `invalid/` 中，它记录预期的错误编号，例如：

```text
start=模块路径::目标名
codes=E001,P001
```

入口文件中的目标只写本地名称，例如 `start=greet`；跨文件目标才写 `模块路径::目标名`。`codes` 列出测试至少要发现的错误编号，工具也可能报告由同一错误引出的其他问题。普通阅读和运行示例时，不需要修改 `expected.txt`。
