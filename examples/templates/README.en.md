# Five Chinese implementation-plan templates

`mkd` organizes Markfile Targets and dependencies into a staged Markdown plan. This directory contains five presentation examples, from a one-page brief to a multi-stage handoff guide. Each `.md.j2` file is a Markdown template with placeholders; the generated document is a regular `.md` file. Templates change presentation only, not the original work specification or prerequisites. See the [custom template guide](../../docs/templates.en.md) for placeholders and loops.

The examples progress from simple to complex. Run the first command, then inspect its `.md.j2` file to see how Target names and acceptance criteria are inserted.

| Order | Template | Demonstrates | Example Target |
| --- | --- | --- | --- |
| 1 | [`01-单目标简报.md.j2`](01-单目标简报.md.j2) | A single task brief and acceptance checklist | `greet` in `01-single-target` |
| 2 | [`02-写作任务清单.md.j2`](02-写作任务清单.md.j2) | Writing tasks and prerequisites grouped by stage | `article` in `02-private-helper` |
| 3 | [`03-并行发布看板.md.j2`](03-并行发布看板.md.j2) | A table of independent work in each stage | `launch` in `07-parallel-launch` |
| 4 | [`04-质量门禁验收.md.j2`](04-质量门禁验收.md.j2) | Acceptance work after multiple prerequisites converge | `ship` in `08-diamond-release` |
| 5 | [`05-流水线交接手册.md.j2`](05-流水线交接手册.md.j2) | A stage-by-stage handoff for a long pipeline | `release` in `06-long-pipeline` |

Run the commands below from the repository root. Outputs go under `/tmp/mkd-templates`; missing output directories are created automatically, so the examples remain unchanged.

First, validate the second template against the `article` Target and its prerequisites. It renders with real data but does not write a file:

```sh
cargo run --quiet --bin mkd -- target article --root examples/valid/02-private-helper/main.mf \
  --template examples/templates/02-写作任务清单.md.j2 --check
```

To generate the first brief, replace `--check` with `-o` and a filename. Open `/tmp/mkd-templates/01.md` to inspect the result:

```sh
cargo run --quiet --bin mkd -- target greet --root examples/valid/01-single-target/main.mf \
  --template examples/templates/01-单目标简报.md.j2 -o /tmp/mkd-templates/01.md
```

Generate the other four documents with:

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

Each `--root` selects an example project's `main.mf`; `target` names a Target in that entry file. If `mkd` is installed, replace `cargo run --quiet --bin mkd --` with `mkd`. When running from another directory, adjust relative `--root`, `--template`, and `-o` paths or use absolute paths. Targets in the same stage can be scheduled in parallel; their order in the generated document is stable for repeatable output. Templates emit descriptions and specifications verbatim, so `|` and other characters may affect Markdown tables.
