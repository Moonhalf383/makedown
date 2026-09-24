# 自定义 Markdown 编译模板

`mkd TARGET --template FILE -o RESULT.md` 使用 `FILE` 渲染指定目标的依赖计划；未指定模板时维持默认 Markdown 格式。模板路径与 `-o` 一样，相对于执行命令时的工作目录，也可以使用绝对路径。

模板采用 MiniJinja 语法：`{{ expression }}` 插入值、`{% for ... %}` 遍历、`{% if ... %}` 分支、`{# ... #}` 注释。支持引擎内建表达式与过滤器，但不提供外部模板加载器或自定义函数；引用其他模板会失败。未知字段、语法错误与渲染错误均阻止写出制品；模板执行有指令额度和递归深度限制。模板内容按 Markdown 原样输出，描述和规格**不会自动转义**。

可用的只读上下文：

| 字段 | 内容 |
| --- | --- |
| `plan.root_target` | 目标的限定名称，例如 `catalog::publish` |
| `plan.stages` | 按依赖排列的阶段列表，基础阶段在前 |
| `stage.index` | 从 1 开始的阶段编号 |
| `stage.targets` | 阶段内按稳定顺序排列的目标列表；同阶段目标互不依赖 |
| `target.id` | 目标的限定名称 |
| `target.name` | 目标的本地名称 |
| `target.dependencies` | 直接前置目标的限定名称列表 |
| `target.description` | 原始目标描述；可能为空字符串 |
| `target.specifications` | 原始规格文本列表；可能为空 |

示例 `plan.md`：

```jinja
# 实施计划：{{ plan.root_target }}

{% for stage in plan.stages %}
## 阶段 {{ stage.index }}

本阶段的目标可以并行实施；进入下一阶段前应完成验收。
{% for target in stage.targets %}
### 目标 `{{ target.id }}`

{% if target.dependencies %}
前置目标：{% for dependency in target.dependencies %}`{{ dependency }}` {% endfor %}
{% endif %}

{{ target.description or '（未提供描述）' }}

{% for specification in target.specifications %}
- [ ] {{ specification }}
{% else %}
（未提供验收规格）
{% endfor %}
{% endfor %}
{% endfor %}
```

执行 `mkd TARGET --check --template FILE` 会校验项目并尝试渲染模板，但不写文件；仅执行 `--check` 时仍沿用项目分析的 dry-run。编译不会覆盖根 Markfile、分析过的其他 Markfile 或所用模板文件。

Neovim 的 `*.md.j2` 高亮及 `mkd-lsp` 语法诊断、字段补全配置见 [`nvim/README.md`](../nvim/README.md)。LSP 不绑定具体目标，因此实际渲染错误仍须通过上述 `--check --template` 检查。
