# 项目配置与初始化

`mkd init [DIRECTORY]` 在当前目录（或指定目录）创建 `main.mf` 和 `mkd.toml`。新项目包含 `start` 目标、默认配方 `main` 和输出 `dist/plan.md`；可以立即运行 `mkd profile`。`--target NAME` 可以给新项目设置本地目标名称。目标目录不存在时自动创建；已有 `main.mf` 但没有 `mkd.toml` 时，必须提供 `--target TARGET`，工具会先分析目标并确认可编译，再生成配置。已有 `mkd.toml` 时不会覆盖，直接报错。

`mkd.toml` 只读取本次选中的入口 Markfile 所在目录的一份：默认入口是向上找到的最近 `main.mf`，也可用 `--root FILE` 显式指定。配置不会改变项目模块路径、lint/fmt 或 LSP 的规则。配置格式版本 `version = 1` 必填，未知字段、错误版本和无效路径会报错。

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

每个 `[build.NAME]` 必须有 `target` 和 `output`，`template` 可选；若有配方，`[build] default` 必须引用其中之一。只含 `version = 1` 的配置也合法，但还不能运行 `mkd profile`。`default` 是 `[build]` 内的保留键，不要将其用作配方名称。每次只编译一个配方的入口目标；不同配方可引用同一目标但分别指定模板和输出。

```sh
mkd profile                # 构建默认配方
mkd profile guide          # 构建指定配方
mkd profile --check        # 检查默认配方和生效模板，不写文件
mkd profile guide -o /tmp/guide.md          # 临时覆盖输出
mkd profile --template alternate.md.j2     # 临时覆盖模板
mkd profile --no-template                   # 使用默认 Markdown 渲染器
```

配置中的 `template`/`output` 必须是**项目目录内的相对文件路径**，禁止绝对路径、越过项目目录的 `..` 以及通过符号链接逃逸；它们以入口 Markfile 所在目录为基准。显式 CLI `-o`/`--template` 仍相对命令启动目录（或可给绝对路径）。构建时会创建缺失的输出父目录，检查模式不会写文件或创建目录。输出不能覆盖入口 Markfile、分析过的其他 Markfile、模板或 `mkd.toml`。

原有的 `mkd TARGET -o FILE` 和 `mkd TARGET --check` 保持独立，**不会读取 `mkd.toml`**；其中 `TARGET` 甚至可以名为 `build`。不打算使用配方时仍可直接构建，且 `lint`/`fmt` 不受构建配置约束。配置模板可用 `mkd profile --check` 验证；若要只检查项目而不使用模板，则用 `mkd profile --check --no-template`。
