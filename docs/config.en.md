# Generate implementation plans from Markfiles

A Markfile is a work specification with the `.mf` extension. Each Target describes a piece of work, its prerequisites, and criteria for deciding when it is complete. `mkd` analyzes the Target dependency graph and generates a staged Markdown plan: prerequisite work appears first, followed by the work that depends on it. Targets within the same stage are independent and may be scheduled in parallel.

## Initialize a project

Run these commands in the directory where you want the project:

```sh
mkd init
mkd build
```

By default, `mkd init` creates `.mkd/main.mf` and an outer `mkd.toml`, and initializes or reuses a Git repository. The `.mkd/` directory keeps Markfiles separate from the host project's source; `mkd.toml` stores build settings. `mkd build` writes the default plan to `dist/plan.md`. To create a new project directory instead:

```sh
mkd init my-project
cd my-project
mkd build
```

## Targets and build profiles

A project may contain many Targets. For example, `main.mf` may define `start`. Run `mkd target start -o plan.md` to compile it directly. Use `-o` to choose the output file. To analyze the Target without writing output, run `mkd target start --check`.

For a repeatable plan, put the Target, output path, and optional formatting template in `mkd.toml`. This reusable configuration is called a build profile. `mkd build` uses the default profile; `mkd build main` explicitly selects the profile named `main`. `mkd target` compiles a command-line Target without consulting `mkd.toml`, which is useful for one-off builds.

Target names come from `.mf` files; profile names come from `mkd.toml`. They can be identical: `mkd target build -o plan.md` compiles the Target named `build`, while `mkd build build` runs the profile named `build`.

## Project roots and initialization

Starting at the current directory, `mkd` searches upward for the nearest project entry. Within each directory, it checks `main.mf`, `lib.mf`, `.mkd/main.mf`, then `.mkd/lib.mf`. In a flat project, Markfiles and configuration sit alongside the entry file. In a hidden-layout project, Markfiles are under `.mkd/`, while `mkd.toml`, `mkd.lock`, and default artifacts are in its parent directory. The `crate::` anchor always starts from the Markfile module directory. Use `--root FILE` to specify an entry explicitly; for a hidden layout, configuration remains in the outer directory.

`mkd init` creates `.mkd/main.mf` by default. `mkd init --flat` creates a flat `main.mf`, suitable for a standalone planning project. `mkd init --lib` creates a flat package project rooted at `lib.mf`. Each option creates `mkd.toml`, a public initial Target named `start`, and a default profile. A directory and `--target launch` can be supplied to customize the project location and initial Target.

If the entry file already exists but `mkd.toml` does not, use the matching layout option and `--target launch` to create configuration. For an existing flat `main.mf`, run `mkd init --flat --target launch`; for an existing `.mkd/main.mf`, run `mkd init --target launch`. The command validates the Target and preserves existing files; it does not overwrite existing configuration.

## Configure `mkd.toml`

Suppose the project contains modules such as `platform` and `content/guide`, and a template at `templates/release.md.j2`. This configuration defines two profiles: `release` is the default, and `guide` can be built separately.

```toml
version = 1

[build]
default = "release"

[build.release]
target = "platform::launch"
template = "templates/release.md.j2"
output = "dist/release.md"

[build.guide]
target = "content::guide::publish"
output = "dist/guide.md"
```

`version = 1` selects the configuration format version. `[build].default` names the profile used by `mkd build` without an explicit profile name. Each `[build.NAME]` section defines a profile with a required `target` and `output`; `template` is optional. If profiles are present, `default` must refer to one of them. The name `default` is reserved for the setting and cannot be used as a profile name.

`platform::launch` refers to Target `launch` in the `platform` module; `content::guide::publish` refers to a Target in a nested module. A Target in the entry file is written with its local name, such as `start`. A module is another `.mf` file used to group Targets. See the [examples](../examples/README.en.md) for module layouts and imports.

Profile paths are relative to the directory containing `mkd.toml` (the outer directory in a hidden layout). They must stay inside the project: absolute paths, paths escaping through parent directories, and symlinks pointing outside the project are rejected. `mkd` validates the configuration, Target names, and paths. A file containing only `version = 1` is a valid empty configuration, but `mkd build` still needs a profile.

## Distributing `.mjar` packages and locking hashes

Package projects use `lib.mf` as their root. Its public section (the `> Target` declarations after the second `---`) is the package interface.

```sh
mkd init --lib shared
cd shared
# Edit lib.mf and other .mf modules
mkd pack                         # dist/shared.mjar
mkd graph --jar dist/shared.mjar # Inspect the archive independently
```

Copy the archive into an application project, for example `.mkd/vendor/shared.mjar` (or `vendor/shared.mjar` in a flat project), and import it from `main.mf`:

```mf
> crate::vendor::*shared::start as shared_start
---
# start
> shared_start
Complete the application Target.
---
> start
```

`*shared` explicitly selects `shared.mjar`; a plain `shared` refers to `shared.mf`. A package and module with the same name cannot occupy the same namespace. Within a package, `crate::` refers to the package itself, while `self` and `super` remain relative. Consumers can import only public Targets from the package's `lib.mf`. Packages preserve module structure and original specification text, so distribution exposes that text and cannot prevent prompt injection embedded in it.

`mkd pack -o FILE` chooses a custom output path. Packaging starts from public Targets and prunes unreachable modules. Imported packages are expanded into the archive; nested `.mjar` files are not retained. The result is validated after writing.

Run `mkd lock` to scan package imports and write `mkd.lock` beside `mkd.toml`, recording package paths and SHA-256 hashes. During later analysis and builds, a changed hash for a package already recorded in the lock file is an error. A missing lock file or a newly added, not-yet-locked package does not prevent building. Run `mkd lock` again after updating packages. Since packaging expands dependencies, the lock file records the `.mjar` files directly referenced by the project. It locks file bytes, not version numbers.

## Inspect the Target dependency graph

`mkd graph` scans the project's Markfiles and renders the Target DAG. Markfiles appear as groups, with arrows from prerequisites to dependent Targets. Each module has a theme color, Targets in one module use related colors, and edge colors blend the endpoint colors.

```sh
mkd graph
mkd graph platform::launch
mkd graph --direction left-right
mkd graph --charset ascii --color never
mkd graph --interactive
mkd graph --image dist/project.svg
mkd graph --format dot -o dist/project.dot
```

Without a Target argument, non-interactive mode shows the whole project, including modules unreachable from the entry Target. With a Target argument, it shows that Target and its recursive prerequisites. `--direction` accepts `top-down`, `bottom-up`, `left-right`, or `right-left` (default: `top-down`). Terminal output uses Unicode line drawing by default; `--charset ascii` selects plain ASCII.

Interactive mode presents one module at a time in a viewport sized to the terminal. It shows all Targets in the current module and their directly referenced external Targets, but does not recursively expand external nodes. It starts at the entry module unless a Target is specified; a specified Target selects its module and becomes the initial focus. All modules remain available regardless of the initial Target.

- `h/j/k/l` or arrow keys: focus a neighboring Target. `Tab`/`Shift-Tab` cycles through nodes.
- `Shift+H/J/K/L` or Shift+arrow keys: pan by one cell (H left, J down, K up, L right; the graph moves in the opposite direction). Pan offset persists when changing focus or modules, within viewport-dependent limits.
- The focused Target has a compact popup with its absolute qualified name and description. It is at most 48 columns by 10 rows, truncates long text, and is excluded from exports.
- `Enter` on a local Target opens its details popup. `Enter` on an external Target enters its module while retaining focus; press `Enter` again for details. The details show description, specifications, and dependencies. Use `j/k` or arrow keys to scroll; `Enter` or `Esc` closes the popup.
- When popups are closed, `Esc`/`Backspace` returns to the previous module and focus. `[`/`]` selects the previous/next module, including modules with no dependency edges.
- `e` exports the current module-local graph without focus highlighting or popups. `Ctrl-C` exits.

For example, if a Target in module A depends on `B::four`, and `B::four` depends on `C::five`, module A shows `B::four`; `C::five` appears after entering module B. Interactive mode requires stdin and stdout to be terminals.

`--image [FILE]` uses the built-in exporter to create a standalone colored SVG; Graphviz is not required. Without a filename it writes `graph.svg` beside the project configuration (beside the archive in `--jar` mode). SVG positions each character explicitly to keep graph lines aligned, including double-width CJK characters. `--format dot` emits Graphviz DOT, which can be converted with tools such as `dot -Tpng`. `-o/--output` writes terminal text or DOT to a file; otherwise output goes to stdout. Automatic color is enabled only when writing directly to a terminal. Graph rendering requires successful project analysis; syntax, import, and dependency-cycle errors prevent rendering.

## Archive project changes

Run `mkd archive` to stage and commit changes. Flat projects stage the project directory; hidden-layout projects stage only `.mkd/`, `mkd.toml`, and an existing `mkd.lock`, so host application source is not included. The command searches upward for the nearest entry when run from a subdirectory.

The commit message summarizes Target changes and lists their qualified names alphabetically:

```text
Archive targets: add 2, update 1, remove 1

Add:
- launch
- catalog

Update:
- quality::test

Remove:
- draft
```

`add`, `update`, and `remove` mean a Target was created, its semantic content changed, or it was deleted. Semantic content includes description, prerequisites, specifications, imports, and public status. If staged changes do not correspond to identifiable Targets (for example, only source files changed or the Markfile cannot be parsed), the message is `Archive project changes`. With no changes, the command fails with `nothing to archive`. Git errors, missing commit identity, and rejected hooks are reported. Configure identity through Git, for example:

```sh
git config user.name "Your Name"
git config user.email "you@example.com"
```

## Common commands

```sh
mkd build                                  # Build the default release profile
mkd build guide                            # Build the guide profile
mkd build --check                          # Validate without writing output
mkd build guide -o guide-copy.md           # Override the output path
mkd build --no-template                    # Use built-in Markdown rendering
mkd target platform::launch -o launch.md  # Build directly, without a profile
mkd target platform::launch --check       # Analyze without writing output
mkd graph platform::launch                 # Show a Target and its prerequisites
mkd graph --interactive                    # Browse modules and Target details
mkd graph --image dist/project.svg         # Export a project SVG
mkd graph --format dot -o dist/project.dot # Export Graphviz DOT
mkd graph --jar dist/shared.mjar           # Inspect a package graph
mkd pack                                   # Package a lib.mf project
mkd lock                                   # Lock referenced package hashes
mkd archive                                # Stage and commit project changes
```

`mkd build --template alternate.md.j2` overrides the template for one build. `mkd target start --template alternate.md.j2 -o plan.md` can specify one directly as well. See [custom Markdown templates](templates.en.md). Command-line `-o` and `--template` paths may be absolute; relative paths are resolved from the current working directory. Paths in `mkd.toml` must be project-relative. `--check` does not create output files or directories. If the effective profile uses a template, `mkd build --check` also renders it using the actual plan data.

When generating a plan, `mkd` creates required output parent directories. It will not overwrite the entry Markfile, other Markfiles read during the build, the active template, or `mkd.toml`. Use `mkd lint .mkd/main.mf` to inspect one file and `mkd lint --all` to inspect all project Markfiles; use `mkd fmt .mkd/main.mf` to format a file. Use `main.mf` instead for a flat layout. These maintenance commands do not require a build profile.
