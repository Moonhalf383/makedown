# Makedown for VS Code

Edit Markfile specifications (`.mf`) and Markdown templates (`.md.j2`) in VS Code. A Markfile Target describes a task, its prerequisites, and acceptance criteria; `mkd` organizes these into a staged Markdown implementation plan. For Markfile syntax and CLI usage, see the [project README](https://github.com/Moonhalf383/makedown#readme).

The extension provides syntax highlighting and bundles the platform-specific `mkd-lsp` language server for diagnostics, completion, and other editor features. Files are checked as you open and edit them, with completion suggestions while typing. `.mf` files also support definition navigation, reference lookup, and formatting. No Rust installation or separate runtime is required. Install from the [Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=Moonhalf383.makedown) or, for Code - OSS, VSCodium, and other Open VSX editors, from the [Open VSX Registry](https://open-vsx.org/extension/Moonhalf383/makedown).

## Features

In `.mf` files:

- Syntax highlighting distinguishes Target titles, `>` imports and prerequisites, `-` acceptance criteria, and `---` separators.
- Diagnostics update as unsaved content changes, underline errors, and list them in the Problems panel.
- Completion is triggered by `>`, `:`, or `.`, including `crate::`, `self::`, and `super::` paths and public Targets.
- Go to Definition (F12) and Find All References (Shift+F12) work on Target names, import paths, and prerequisites.
- Format Document is available from the context menu or with Shift+Alt+F.

In `.md.j2` templates, Markdown highlighting is combined with MiniJinja syntax highlighting (Jinja-like `{{ placeholder }}` expressions). The language service reports template syntax errors (`T001`) and unknown `plan` fields (`T002`), and completes fields after `plan.`, `stage.`, and `target.`.

The extension is for editing. To validate or generate a plan, or render a template against real project data, use `mkd` in a terminal, for example `mkd target release -o plan.md` or `mkd target release --check --template plan.md.j2`.

## Build locally

Building requires Rust, Node.js 22 or later, npm, and platform build tools. From the repository root, run:

```sh
make vscode-deps
make vscode-test
make vscode-package
```

The final command builds the Rust language server, copies its executable and the repository's MIT license into the extension directory, then packages the extension with `vsce`. Do not bypass it by running `vsce package` directly. Install the generated `vscode/*.vsix` using the VS Code Extensions view's **Install from VSIX...** command.

## Supported platforms

VSIX packages are built for six desktop targets: Windows, macOS, and Linux on x64 and ARM64. VS Code for the Web, 32-bit systems, and Alpine Linux are not supported. For remote development, the extension runs on the remote machine where the workspace is located; install a VSIX matching that machine's platform. Report issues through [GitHub Issues](https://github.com/Moonhalf383/makedown/issues).
