# Explore Markfile projects through examples

A Markfile is a work specification with the `.mf` extension. A file can define multiple Targets, each describing a task, its prerequisites, and acceptance criteria. `mkd` turns this information into a staged Markdown implementation plan.

Start with [`valid/01-single-target/main.mf`](valid/01-single-target/main.mf). It defines a Target named `greet`: `# greet` begins the Target, ordinary text describes the work, and lines beginning with `-` are acceptance criteria. The final `> greet` exports it so other Markfiles can reference it. From the repository root, inspect and compile the example:

```sh
cargo run --quiet --bin mkd -- target greet --root examples/valid/01-single-target/main.mf --check
cargo run --quiet --bin mkd -- target greet --root examples/valid/01-single-target/main.mf -o /tmp/greet-plan.md
```

`cargo run --quiet --bin mkd --` runs the program from source. If `mkd` is installed, replace that prefix with `mkd`. `--root` selects the project entry file; `--check` analyzes without writing, and `-o` selects the output path. See [project configuration](../docs/config.en.md) for more commands and settings.

Examples in `valid/` progressively introduce prerequisites, cross-file imports, nested modules, and parallel stages. A module is another `.mf` file that groups Targets. For example, `catalog::publish` names Target `publish` in module `catalog`. For alternative plan layouts, see the [Chinese template examples](templates/README.en.md).

The `invalid/` directory contains common errors, such as duplicate Targets, missing modules, and dependency cycles. Each example's `expected.txt` is a compact fixture for automated tests. Its first line specifies the starting Target. In `valid/`, it also records the expected stage count, such as `stages=2`; in `invalid/`, it records expected diagnostic codes, for example:

```text
start=module::target
codes=E001,P001
```

A Target in the entry Markfile uses a local name, such as `start=greet`; Targets from other modules use `module::target`. `codes` lists errors the test must detect. The tool may also report additional errors caused by the same underlying problem. You normally do not need to edit `expected.txt` when reading or running examples.
