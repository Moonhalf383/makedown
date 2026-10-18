# Edit Markfiles in Neovim

A Markfile (`.mf`) is a work specification. Each Target describes work, prerequisites, and acceptance criteria; `mkd` can turn these into a staged Markdown plan. This directory provides syntax highlighting and language-server support for Neovim. Highlighting makes file structure easier to scan; `mkd-lsp` reports errors and offers completion, navigation, and formatting while editing. The setup requires Neovim 0.11 or later.

## Install and enable

Build the language server from the repository root:

```sh
cargo build --bin mkd-lsp
```

This creates `target/debug/mkd-lsp`. With LazyVim, load the project configuration after LazyVim in `~/.config/nvim/init.lua`, replacing the path with the absolute path to this repository:

```lua
require("config.lazy")
dofile("/path/to/makedown/nvim/lsp.lua")
```

`nvim/lsp.lua` adds this repository's syntax files to Neovim's runtime path, recognizes `.mf` and `.md.j2`, and starts the built `mkd-lsp`. Keep `dofile(...)` after `require("config.lazy")` so LazyVim does not overwrite the added runtime path. If the server does not start, make sure the build completed and `target/debug/mkd-lsp` exists.

To use highlighting only, you do not need to build or start the language server. Add this repository's `nvim` directory to Neovim's runtime path, then enable filetype detection and syntax highlighting, for example with `:filetype on` and `:syntax on`. The LazyVim setup above enables both highlighting and LSP features.

## Check `.mf` highlighting

From the repository root, open `examples/valid/01-single-target/main.mf` and ensure `:syntax on` is enabled. The second line contains the Target title `# greet`. Run:

```vim
:set filetype?
:lua print(vim.inspect(vim.api.nvim_get_runtime_file("syntax/mf.vim", true)))
:lua print(vim.fn.synIDattr(vim.fn.synID(2, 1, 1), "name"))
```

The first listed syntax path should be this project's `nvim/syntax/mf.vim`. The final command checks line 2, column 1 and should return `mfTargetMarker`. Change `2` to the line containing `# ...` when checking another file. This path check matters because Neovim also ships a `syntax/mf.vim` for Metafont; a `filetype` of `mf` alone does not guarantee that this project's syntax rules were loaded.

## Edit Targets

The language server analyzes an opened or edited `.mf` file, including unsaved changes. Completion is available for imported Targets, prerequisites, and public Target declarations. Place the cursor on a Target name, import path, or prerequisite and run `:lua vim.lsp.buf.definition()` to jump to its definition or `:lua vim.lsp.buf.references()` to list references.

This configuration enables completion automatically. Typing `>`, `:`, or a space can trigger candidates; you can also request them with Ctrl-X followed by Ctrl-O (`<C-x><C-o>`). A space after `>` can help Neovim show the candidates again. Select with `<C-n>` or `<C-p>`. The preview shows a Target's description, prerequisites, and specifications, or a module's public Targets.

Path completion supports `crate::` (from the project entry), `self::` (from the current file's directory), and `super::` (from the parent directory). Import candidates include these anchors, public Targets, and module paths. Prerequisite candidates include local Targets and imported aliases. For example, `> crate::shared::lint as shared_lint` gives the imported Target a local alias; completion does not currently suggest alias names.

The completion menu uses `menuone,noselect,popup`, so it does not select or insert the first candidate automatically. Some Neovim configurations need an extra documentation-preview window after candidate selection; `lsp.lua` handles this. If the optional Noice plugin is installed with its external completion menu enabled, the preview is placed beside the menu to avoid being obscured.

Format the current `.mf` buffer with:

```vim
:lua vim.lsp.buf.format()
```

The language server returns edits to Neovim rather than writing to disk. Review the result in the editor and save when ready.

## Edit Markdown templates

Files ending in `.md.j2` are Markdown templates for implementation plans and may contain MiniJinja placeholders such as `{{ ... }}`. They use the `mkd_template` filetype, layering template syntax highlighting over Markdown. Ordinary `.j2` files remain under other configuration. See the [template guide](../docs/templates.en.md) for syntax and fields.

Open `examples/templates/02-写作任务清单.md.j2` and run:

```vim
:set filetype?
:lua print(vim.fn.synIDattr(vim.fn.synID(2, 1, 1), "name"))
:lua print(vim.fn.synIDattr(vim.fn.synID(5, 1, 1), "name"))
```

The filetype should be `mkd_template`; the two highlight checks should return `markdownH1Delimiter` and `mkdTemplateStatement`. The language server reports template syntax errors (`T001`) and explicitly written unknown `plan` fields (`T002`). Typing a dot after `plan.`, `stage.`, or `target.` inside a template tag offers field completion. These names refer to the full plan, current stage, and current Target, respectively. Manual completion with `<C-x><C-o>` is also available.

To test a template with real project data, run:

```sh
mkd target start --check --template plan.md.j2
```

Replace `start` with your Target and `plan.md.j2` with the template path. This attempts a render but writes no file. Editor diagnostics check only some static template constructs; they do not render against the project's Targets. Template formatting, definition navigation, and reference lookup are not implemented.

## Scope and implementation

For Markfile diagnostics, inspect the reported `.mf` file. The language server analyzes files imported by the open document and reports missing modules at import sites; if an imported file has errors, open that file to see its diagnostics. `mkd lint --all` checks every `.mf` in a project, including files not imported by another module. Completion and reference lookup use a project-wide file index and account for unsaved editor content.

The index is rebuilt on a subsequent request after a document changes or closes. Changes made by external programs are not immediately reflected. Incremental indexing and rename support are not yet available. Highlight rules match line-leading markers, so text in a description may occasionally look like a structural marker; diagnostics use the actual parser.

Implementation entry points: `src/bin/mkd-lsp.rs` starts the language server; `src/lsp.rs` handles editor requests and unsaved content; `src/lsp/workspace.rs` provides cross-file lookup. The server and CLI share rules from `src/parser.rs`, `src/linter.rs`, `src/formatter.rs`, and `src/project.rs`. Filetype detection is in `nvim/ftdetect/`, and highlighting rules are in `nvim/syntax/`.
