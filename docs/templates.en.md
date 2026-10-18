# Customize Markdown rendering

By default, `mkd` turns Targets and their prerequisites into a staged Markdown implementation plan. You can write a template to produce a checklist, handoff record, or format used by your team. A template is a Markdown file with placeholders. During compilation, `mkd` fills them with Target names, stages, and specification text. Templates change presentation only; they do not change dependencies or stage order.

In a project initialized with `mkd init`, save the example below as `plan.md.j2` and run:

```sh
mkd target start --template plan.md.j2 -o plan.md
```

Relative `--template` and `-o` paths are resolved from the current working directory; absolute paths are also accepted. To reuse a template, configure it in a [build profile](config.en.md) and run `mkd build`. See the [template examples](../examples/templates/README.en.md) for complete commands.

## Access plan data

Templates use MiniJinja syntax. `{{ plan.root_target }}` inserts the selected root Target name. A compilation produces a `plan` with multiple `stage` objects; each stage contains `target` objects. Use `plan.stages` to iterate through stages and `stage.targets` to iterate through a stage's Targets.

Save this as `plan.md.j2`. The outer `for` loop emits stages, and the inner loop emits Targets in each stage. `if` checks whether a Target has prerequisites.

```jinja
# Implementation plan: {{ plan.root_target }}

{% for stage in plan.stages %}
## Stage {{ stage.index }}

{% for target in stage.targets %}
### {{ target.name }}

{% if target.dependencies %}
Prerequisites: {% for dependency in target.dependencies %}{{ dependency }} {% endfor %}
{% endif %}

{{ target.description or '(No description provided)' }}

{% for specification in target.specifications %}
- [ ] {{ specification }}
{% else %}
- (No acceptance criteria provided)
{% endfor %}
{% endfor %}
{% endfor %}
```

If `start` depends on `draft`, the template lists the stage containing `draft` before the later stage containing `start`. Multiple independent Targets in a stage may be worked on in parallel; their displayed order is stable, not a prescribed execution order. `target.specifications` contains the acceptance criteria from the Markfile.

Available fields are listed below. Use scalar fields in `{{ ... }}` and iterate over lists with `for` loops.

| Field | Meaning |
| --- | --- |
| `plan.root_target` | Qualified name of the selected Target, for example `catalog::publish` |
| `plan.stages` | Stages ordered from prerequisites to dependent work |
| `stage.index` | One-based stage number |
| `stage.targets` | Targets in the current stage; they are mutually independent |
| `target.id` | Qualified Target name, for example `catalog::publish` |
| `target.name` | Target's local name in its Markfile, for example `publish` |
| `target.dependencies` | Qualified names of direct prerequisite Targets |
| `target.description` | Original Target description; empty when absent |
| `target.specifications` | Original acceptance criteria; may be empty |

MiniJinja supports template comments using `{# ... #}`, along with common expressions and filters. Descriptions and specifications are emitted as-is; characters such as `|` are not escaped for Markdown tables. Consider whether source text can break table formatting before placing it in a table. Templates cannot include other templates or call project-defined functions. The renderer limits execution fuel and recursion depth.

## Validate before generating

You can render with real plan data without writing a file:

```sh
mkd target start --check --template plan.md.j2
```

This checks the project and attempts to render the template. Invalid fields, syntax errors, and rendering failures are reported. Fix them before running the command with `-o`. By contrast, `mkd target start --check` without a template checks Targets and dependencies but does not render a template. When writing output, `mkd` prevents overwriting Markfiles read by the build or the active template.

In Neovim, see the [editor setup](../nvim/README.en.md) to enable highlighting, template diagnostics, and field completion for `.md.j2` files. Static editor checks catch only some errors; `--check` with the template is the authoritative test using actual plan data.
