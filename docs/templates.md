# 定制 Markdown 排版

`mkd` 默认会把目标及其依赖生成按阶段排列的 Markdown 实施计划。如果需要任务清单、交接记录或团队惯用的标题格式，可以编写一个模板。模板是一份带有占位符的 Markdown 文件：编译时，`mkd` 用实际的目标名称、阶段和规格文本填入占位符，再写出最终的 `.md` 文件。模板改变的是呈现方式，不会改变目标之间的依赖关系或阶段顺序。

例如，在已经运行过 `mkd init` 的项目根目录，将下文的示例模板保存为 `plan.md.j2`，就可以用它排版 `start` 目标，并把结果写入 `plan.md`：

```sh
mkd target start --template plan.md.j2 -o plan.md
```

`--template` 和 `-o` 后的相对路径以运行命令时所在的目录为起点，也可以使用绝对路径。重复使用同一模板时，可在 `mkd.toml` 中把它写进[构建配方](config.md)，然后运行 `mkd build`。模板示例及完整运行命令见[中文模板示例](../examples/templates/README.md)。

## 读取计划中的信息

模板使用 MiniJinja 语法。最简单的写法是 `{{ plan.root_target }}`，它会插入正在编译的目标名称。例如，`# 实施计划：{{ plan.root_target }}` 可能生成标题 `# 实施计划：start`。一次构建会得到一个计划 `plan`，其中包含多个阶段 `stage`；每个阶段又包含若干目标 `target`。`plan.stages` 用来取得阶段列表，`stage.targets` 用来取得一个阶段里的目标。

下面是一份可保存为 `plan.md.j2` 的模板。`for` 表示逐项列出：外层依次写出阶段，内层写出该阶段的目标。`if` 用来判断目标是否有前置工作。

```jinja
# 实施计划：{{ plan.root_target }}

{% for stage in plan.stages %}
## 阶段 {{ stage.index }}

{% for target in stage.targets %}
### {{ target.name }}

{% if target.dependencies %}
前置目标：{% for dependency in target.dependencies %}{{ dependency }} {% endfor %}
{% endif %}

{{ target.description or '（未提供描述）' }}

{% for specification in target.specifications %}
- [ ] {{ specification }}
{% else %}
- （未提供验收规格）
{% endfor %}
{% endfor %}
{% endfor %}
```

假设 `start` 依赖一个名为 `draft` 的目标，这份模板会先列出 `draft` 所在的基础阶段，再列出 `start` 所在的后续阶段。一个阶段里如果有几个相互独立的目标，它们可以并行实施，文件中的排列只用于稳定展示；具体顺序由实施者决定。`target.specifications` 取自工作说明中的验收条目，因此上面的复选框可用作实施时的核对清单。

模板可以读取以下信息。表中的名称可放在 `{{ ... }}` 中输出；列表通常放在 `for` 循环中逐项处理。

| 名称 | 含义 |
| --- | --- |
| `plan.root_target` | 本次编译的目标名称，例如 `catalog::publish` |
| `plan.stages` | 先基础工作、后依赖工作的阶段列表 |
| `stage.index` | 阶段编号，从 1 开始 |
| `stage.targets` | 当前阶段的目标列表；同一阶段的目标互不依赖 |
| `target.id` | 带模块路径的完整目标名称，例如 `catalog::publish` |
| `target.name` | 目标在所属文件中的本地名称，例如 `publish` |
| `target.dependencies` | 直接依赖的目标名称列表 |
| `target.description` | 目标的原始描述文本；没有描述时为空 |
| `target.specifications` | 目标的原始验收条目列表；可能为空 |

MiniJinja 还支持用 `{# ... #}` 写模板注释，以及常见的表达式和过滤器。模板会原样输出工作说明中的描述和规格，不会自动把其中的竖线等符号处理成适合 Markdown 表格单元格的文本。如果要把描述放进表格，请先考虑原始文本是否可能破坏表格排版。模板不能加载其他模板，也没有项目自定义函数；引擎对执行量和递归深度设有限制。

## 先检查，再生成

可以先尝试生成内容但不写文件：

```sh
mkd target start --check --template plan.md.j2
```

这条命令会检查项目，并用实际的计划数据尝试渲染模板。模板字段写错、语法错误或渲染出错时，会报告错误；修好后再运行带 `-o` 的命令即可。只运行 `mkd target start --check` 时，工具检查目标和依赖，不渲染模板。生成文档时，工具会保护所读取的 `.mf` 文件和模板文件，不允许把输出写到这些文件上。

在 Neovim 中编辑 `.md.j2` 文件时，可按[编辑器配置说明](../nvim/README.md)启用高亮、语法诊断和字段补全。编辑器可以帮助发现部分输入错误；实际输出仍以带模板的 `--check` 为准。
