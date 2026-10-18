<p align="center">
  <img src="https://raw.githubusercontent.com/Moonhalf383/makedown/main/assets/icon.png" alt="Makedown icon" width="192">
</p>

# Makedown

**Makefile + Markdown = Markfile + Makedown**

Makedown is a command-line tool that turns work specifications into implementation plans. Write goals, their prerequisites, and completion criteria in Markfiles (`.mf` files). `mkd` analyzes them and produces a staged Markdown document. Prerequisite work comes first; goals in the same stage are independent and can be scheduled in parallel. The resulting plan can be handed to teammates or AI assistants.

Read on for the [examples](examples/README.en.md), [project configuration](docs/config.en.md), [template guide](docs/templates.en.md), [Neovim setup](nvim/README.en.md), or [VS Code extension](vscode/README.en.md).

## Install

Install the CLI from crates.io:

```sh
cargo install makedown-cli --locked
```

This installs two commands: `mkd` validates Markfiles and generates plans; `mkd-lsp` provides editor diagnostics, completion, navigation, and formatting. Verify the installation with `mkd --version`. Prebuilt binaries are also available from [GitHub Releases](https://github.com/Moonhalf383/makedown/releases).

## A Markfile at a glance

This example describes assembling and approving a release package. A Markfile has three sections: imports at the top, the target body between `---` separators, and public exports after the second separator.

```makedown
> super::build::backend::package_backend
> super::build::frontend::package_frontend
> super::quality::gate::approve
---
# release
> package_backend
> package_frontend
> approve
Assemble and approve the complete release package.
- Frontend and backend artifacts use the same contract version.
- Archive the quality gate results with the release package.
---
> release
```

A target body begins with `# release`. Ordinary text below the title describes the work; lines beginning with `-` are acceptance criteria. Prerequisites are written with `>` between the title and description. `mkd` schedules those prerequisites before `release`, producing a staged plan.

At the top of the file, `super::` is a path anchor: it starts in the parent directory, and each `::` continues into a module file. For example, `super::quality::gate::approve` imports target `approve` from `quality/gate.mf` in the parent directory, making it available locally under its original name. A `>` at the top is an import; one inside a target body is a prerequisite. Other anchors include `crate::` (the project entry file) and `self::` (the current file's directory). Imported targets must be exported in their own file's public section.

Projects can contain multiple Markfiles. Application projects use `main.mf`; package projects use `lib.mf`. By default, application Markfiles live under `.mkd/`. See the [examples](examples/README.en.md) for complete projects.

## Run an example

After installing `mkd`, run these commands from the repository root:

```sh
mkd target greet --root examples/valid/01-single-target/main.mf --check
mkd target greet --root examples/valid/01-single-target/main.mf -o greeting-plan.md
```

The first command checks the project without writing a file. The second writes `greeting-plan.md` in the repository root. Change the output path to a temporary location if you do not want an artifact in the repository, for example `/tmp/greeting-plan.md` (or `%TEMP%\greeting-plan.md` on Windows).

`target greet` selects the target to compile. `--root` specifies the example project's entry file. In your own project, `mkd` searches upward from the current directory for the nearest supported entry file.

## Create a project

Run this in the directory where you want the project:

```sh
mkd init my-project
cd my-project
mkd build
```

This creates `.mkd/main.mf`, an outer `mkd.toml`, initializes or reuses a Git repository, and writes the generated plan to `dist/plan.md`. Use `mkd init --flat` for a flat layout, or `mkd init --lib` for a flat package project rooted at `lib.mf`. The initial target is named `start`. See the [configuration guide](docs/config.en.md) for initialization options and configuration details.

The generated `main.mf` has the same three sections shown above, with an empty import section. Replace the sample text with your own work and add prerequisites as needed.

## Choose how to build

`mkd target start -o plan.md` compiles target `start` directly. Use `mkd target start --check` to analyze it without writing an artifact. Targets can be qualified, for example `catalog::publish`; target paths on the command line are relative to the project entry and do not need a `crate::` prefix.

For repeatable builds, define a build profile in `mkd.toml` with a target, output path, and optional template. `mkd build` uses the default profile; `mkd build NAME` selects a named profile. `mkd target` takes the target and output from its command line instead of reading profiles. Use `mkd lint .mkd/main.mf` to lint one file, or `mkd fmt .mkd/main.mf` to format it (use `main.mf` for a flat project).

## Packages and imports

A project rooted at `lib.mf` can be packaged with `mkd pack`, producing `dist/<project-name>.mjar`. The archive preserves module structure, and its public interface is the export section of `lib.mf`. Copy it to `.mkd/vendor/utils.mjar` in another project and import it with `> crate::vendor::*utils::target as alias`. The `*utils` marker distinguishes a package from a regular `utils.mf` module. Packaging expands imported packages; nested archives are not retained, and specification text remains visible. `mkd lock` records SHA-256 hashes of referenced packages and detects changes to hashes already in the lock file. View an archive independently with `mkd graph --jar dist/<project-name>.mjar`. See [package and lock details](docs/config.en.md#distributing-mjar-packages-and-locking-hashes).

## Visualize dependencies

`mkd graph` renders the project's Target dependency graph in the terminal. Each Markfile is a labeled group, each Target is a node, and arrows point from prerequisites to the targets that depend on them. Targets in a module use related colors, while modules have distinct theme colors. Edge colors blend the colors of their endpoints. ANSI color follows the global `--color auto|always|never` setting and is disabled by default when output is redirected.

```sh
mkd graph                                      # Unicode, top-to-bottom project graph
mkd graph platform::launch                     # Target and its prerequisites
mkd graph --direction left-right               # Left-to-right layout
mkd graph --charset ascii --color never        # Plain-text output
mkd graph --interactive                        # Browse modules and inspect Targets
mkd graph --image project.svg                  # Export SVG directly
mkd graph --format dot -o project.dot          # Export Graphviz DOT
mkd graph --jar dist/example.mjar              # View a package graph
```

Interactive mode shows one module at a time. It initially shows every Target in that module and their directly referenced external Targets; external prerequisites expand when you enter their module. Use `h/j/k/l` or arrow keys to move focus, `Tab`/`Shift-Tab` to cycle through nodes, and `Shift+H/J/K/L` (or Shift+arrow keys) to pan the view. A compact popup follows the focused Target and shows its qualified name and description. Press `Enter` on a local Target to open its details; press `Enter` on an external Target to enter its module, then press it again to open details. The details popup includes description, specifications, and dependencies. Use `j/k` or arrow keys to scroll it.

When popups are closed, `Esc`/`Backspace` returns to the previous module and focus. `[`/`]` switches modules, including modules without dependency edges. `e` exports the current module's local graph to `graph.svg` by default; `Ctrl-C` exits. Set a different export path with `mkd graph --interactive --image dist/project.svg`. SVG export is built in and does not require Graphviz. `--direction` also accepts `bottom-up` and `right-left`; DOT output can be converted with Graphviz, for example `dot -Tpng project.dot -o project.png`. Graph rendering requires successful project analysis.

## Archive project changes

Run `mkd archive` to stage and commit project changes. A flat project stages its entire directory. A hidden-layout project stages only `.mkd/`, `mkd.toml`, and an existing `mkd.lock`, leaving host application source files untouched. The commit message summarizes added, updated, and removed Targets, then lists them by action. If only ordinary project files changed, it uses `Archive project changes`. Before the first commit, configure Git's `user.name` and `user.email` as usual. See the [configuration guide](docs/config.en.md) for command and path rules.

Generated plans use built-in Markdown formatting by default. To create a custom checklist or handoff document, see the [template guide](docs/templates.en.md) and the [five Chinese template examples](examples/templates/README.en.md). For editor support, see the [Neovim](nvim/README.en.md) or [VS Code](vscode/README.en.md) guide.
