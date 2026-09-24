# 五种中文实施计划模板

`mkd` 会把 Markfile 中的目标和依赖整理成分阶段的 Markdown 实施计划。本目录提供五种排版示例，从一页简报到多阶段交接手册。每份模板都是带有占位符的 Markdown 文件，扩展名为 `.md.j2`；生成的文档是普通的 `.md` 文件。模板只改变呈现方式，不修改原来的工作说明和前置关系。占位符和循环的写法见[自定义模板说明](../../docs/templates.md)。

这些例子由简单到复杂。建议先运行第一个，再打开对应的 `.md.j2` 文件，看看目标名称和验收条目是如何填入文档的。

| 顺序 | 模板 | 演示内容 | 示例目标 |
| --- | --- | --- | --- |
| 1 | [`01-单目标简报.md.j2`](01-单目标简报.md.j2) | 把单项工作的说明排成简报和验收清单 | `01-single-target` 中的 `greet` |
| 2 | [`02-写作任务清单.md.j2`](02-写作任务清单.md.j2) | 逐阶段列出写作任务和前置工作 | `02-private-helper` 中的 `article` |
| 3 | [`03-并行发布看板.md.j2`](03-并行发布看板.md.j2) | 把同阶段可并行的工作排成表格 | `07-parallel-launch` 中的 `launch` |
| 4 | [`04-质量门禁验收.md.j2`](04-质量门禁验收.md.j2) | 汇总多条前置工作汇合后的验收事项 | `08-diamond-release` 中的 `ship` |
| 5 | [`05-流水线交接手册.md.j2`](05-流水线交接手册.md.j2) | 为较长的工作链生成逐阶段交接记录 | `06-long-pipeline` 中的 `release` |

下面的命令从仓库根目录运行，生成的计划放在 `/tmp/mkd-templates`。工具会自动创建缺失的输出目录，因此不会改动示例项目。

可以先检查第二个模板。这个命令会读取写作项目中的 `article` 目标及前置工作，尝试用模板生成内容，但不写出文件：

```sh
cargo run --quiet --bin mkd -- target article --root examples/valid/02-private-helper/main.mf \
  --template examples/templates/02-写作任务清单.md.j2 --check
```

生成第一份简报时，把 `--check` 换成 `-o` 和文件名。完成后打开 `/tmp/mkd-templates/01.md` 查看排版：

```sh
cargo run --quiet --bin mkd -- target greet --root examples/valid/01-single-target/main.mf \
  --template examples/templates/01-单目标简报.md.j2 -o /tmp/mkd-templates/01.md
```

其余四份可以按顺序生成，对照模板文件和最终文档来理解阶段、前置目标和验收条目的呈现方式：

```sh
cargo run --quiet --bin mkd -- target article --root examples/valid/02-private-helper/main.mf \
  --template examples/templates/02-写作任务清单.md.j2 -o /tmp/mkd-templates/02.md
cargo run --quiet --bin mkd -- target launch --root examples/valid/07-parallel-launch/main.mf \
  --template examples/templates/03-并行发布看板.md.j2 -o /tmp/mkd-templates/03.md
cargo run --quiet --bin mkd -- target ship --root examples/valid/08-diamond-release/main.mf \
  --template examples/templates/04-质量门禁验收.md.j2 -o /tmp/mkd-templates/04.md
cargo run --quiet --bin mkd -- target release --root examples/valid/06-long-pipeline/main.mf \
  --template examples/templates/05-流水线交接手册.md.j2 -o /tmp/mkd-templates/05.md
```

每条命令中的 `--root` 指定示例项目的入口 `main.mf`，随后 `target` 后面填写这个入口文件中的目标名称。如果已安装 `mkd`，可将命令开头的 `cargo run --quiet --bin mkd --` 换成 `mkd`。从别的目录运行时，`--root`、`--template` 和 `-o` 后的相对路径都要按当时的工作目录调整，也可以改用绝对路径。同一阶段中的目标可以并行安排；文档里的排列顺序只为了让每次生成的结果一致。模板原样输出目标的说明和规格，如果把这些文本放入 Markdown 表格，需留意竖线等字符可能改变表格结构。
