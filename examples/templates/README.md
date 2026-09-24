# 中文自定义模板示例

以下模板按复杂度从低到高排列。模板只负责将已有编译计划排版为 Markdown，不会改变 Markfile 中的依赖关系、阶段顺序或验收规格。模板文件使用 `.md.j2` 扩展名以区别最终生成的 `.md` 制品。语法及字段见 [`../../docs/templates.md`](../../docs/templates.md)。

下列命令均从**仓库根目录**运行；若使用已安装的程序，可将 `cargo run --quiet --bin mkd --` 换成 `mkd`。输出放在临时目录，以免改动示例项目：

```sh
mkdir -p /tmp/mkd-templates
```

| 级别 | 文件与场景 | 对应项目及入口 |
| --- | --- | --- |
| 1 | [`01-单目标简报.md.j2`](01-单目标简报.md.j2)：直接插值与最小验收列表 | `01-single-target` / `greet` |
| 2 | [`02-写作任务清单.md.j2`](02-写作任务清单.md.j2)：阶段遍历、前置条件与空规格兜底 | `02-private-helper` / `article` |
| 3 | [`03-并行发布看板.md.j2`](03-并行发布看板.md.j2)：并行任务表格、计数和条件提示 | `07-parallel-launch` / `launch` |
| 4 | [`04-质量门禁验收.md.j2`](04-质量门禁验收.md.j2)：菱形依赖、直接前置目标及累计统计 | `08-diamond-release` / `ship` |
| 5 | [`05-流水线交接手册.md.j2`](05-流水线交接手册.md.j2)：长链路阶段目录与逐项交接记录 | `06-long-pipeline` / `release` |

先试运行模板（只检查，不写文件），例如：

```sh
cargo run --quiet --bin mkd -- article --root examples/valid/02-private-helper/main.mf \
  --template examples/templates/02-写作任务清单.md.j2 --check
```

分别生成五份文档：

```sh
cargo run --quiet --bin mkd -- greet --root examples/valid/01-single-target/main.mf \
  --template examples/templates/01-单目标简报.md.j2 -o /tmp/mkd-templates/01.md
cargo run --quiet --bin mkd -- article --root examples/valid/02-private-helper/main.mf \
  --template examples/templates/02-写作任务清单.md.j2 -o /tmp/mkd-templates/02.md
cargo run --quiet --bin mkd -- launch --root examples/valid/07-parallel-launch/main.mf \
  --template examples/templates/03-并行发布看板.md.j2 -o /tmp/mkd-templates/03.md
cargo run --quiet --bin mkd -- ship --root examples/valid/08-diamond-release/main.mf \
  --template examples/templates/04-质量门禁验收.md.j2 -o /tmp/mkd-templates/04.md
cargo run --quiet --bin mkd -- release --root examples/valid/06-long-pipeline/main.mf \
  --template examples/templates/05-流水线交接手册.md.j2 -o /tmp/mkd-templates/05.md
```

阶段内目标虽按稳定顺序展示，但并不要求按此顺序实施；实际并行安排由实施者决定。模板按原文输出描述与规格，不能将它们当作已转义的 Markdown 表格单元格。若从其他目录调用，请将 `--root`、`--template` 和 `-o` 的路径改为相应绝对路径或相对于该工作目录的路径。
