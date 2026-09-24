# 从 Markfile 生成实施计划

Markfile 是扩展名为 `.mf` 的工作说明文件。文件里的一个目标（Target）描述一项要完成的工作，可以写明它依赖哪些工作，以及怎样判断工作已经完成。`mkd` 会读取目标及其依赖，生成按阶段排列的 Markdown 实施计划：先安排基础工作，再安排依赖这些工作的后续任务。同一阶段的目标之间没有依赖关系，可以由实施者并行安排。

第一次使用时，可以让 `mkd` 帮你创建一个项目。在准备存放项目的目录里运行：

```sh
mkd init
mkd build
```

在当前目录创建项目时，第一条命令创建 `main.mf` 和 `mkd.toml`。`main.mf` 是项目的入口工作说明文件，`mkd.toml` 保存常用的构建设置。第二条命令读取设置，生成 `dist/plan.md`；打开这个文件就能看到实施计划。如果想让工具新建一个项目目录，可以改用以下步骤：

```sh
mkd init my-project
cd my-project
mkd build
```

## 目标和构建配方

一个项目通常有多个目标。例如 `main.mf` 中可以有一个名为 `start` 的目标。`mkd target start -o plan.md` 会直接生成这个目标的实施计划，并写入 `plan.md`。这里的 `-o` 指定输出文件。检查内容是否能成功分析、暂时不生成文件时，可以运行 `mkd target start --check`。

如果经常要生成同一份计划，可以把目标、输出文件以及可选的排版模板写进 `mkd.toml`。这组可重复使用的设置叫构建配方（build profile）。例如配方 `main` 可以指定目标 `start`，将计划写到 `dist/plan.md`。此时运行 `mkd build` 就会使用默认配方；运行 `mkd build main` 则明确指定名为 `main` 的配方。`mkd target` 按命令行给出的目标编译，不读取 `mkd.toml`，适合临时指定输出；`mkd build` 读取配方，适合重复生成固定文档。

目标名来自 `.mf` 文件，配方名来自 `mkd.toml`。两者可以相同：要编译名为 `build` 的目标，写 `mkd target build -o plan.md`；要执行名为 `build` 的配方，写 `mkd build build`。

## 项目根目录和初始化

运行 `mkd build` 或 `mkd target` 时，工具默认从当前目录向上寻找最近的 `main.mf`，并以该文件所在目录作为项目根目录。例如从 `my-project/notes` 目录运行 `mkd build`，如果 `my-project/main.mf` 是最近的入口文件，就会使用 `my-project/mkd.toml`。若入口文件另有名字，可通过 `--root` 指定它。例如，在 `my-project` 目录里使用 `mkd build --root entry.mf`。构建配方仍从所选入口文件旁边的 `mkd.toml` 读取。

运行 `mkd init` 时，新项目默认创建名为 `start` 的本地目标，以及名为 `main` 的默认配方。可以用 `mkd init --target launch` 给新目标指定名称，也可以用 `mkd init my-project --target launch` 在指定目录创建它。这里的“本地”表示目标就在新建的 `main.mf` 里。

如果某个目录里已有 `main.mf`，但尚无 `mkd.toml`，先确认入口文件中确实有 `launch` 目标，再在该目录运行 `mkd init --target launch`。工具会检查该目标能否成功分析，为它创建配置，并保留原有的 `main.mf`。已有 `mkd.toml` 时，初始化会报错，以免覆盖配置。

## 编写 mkd.toml

假设项目中已有 `platform` 和 `content/guide` 等模块文件，并准备了 `templates/release.md.j2` 模板，就可以使用下面这份配置。它提供两份常用计划：`release` 是默认配方，`guide` 是另一份可单独构建的配方。只想先体验完整流程时，使用前面的 `mkd init` 即可，它会创建可以直接运行的简单配置。

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

`version = 1` 声明配置格式版本。`[build]` 中的 `default` 指定不带配方名运行 `mkd build` 时使用哪一个配方。`[build.release]` 和 `[build.guide]` 分别保存两个配方，方括号中的后半部分就是配方名。每个配方需要一个 `target` 和一个 `output`；`template` 可以省略，省略后会使用内置的 Markdown 排版。配置了配方时，`default` 必须指向其中一个。`default` 用作设置名称，不能再作为配方名。

`platform::launch` 表示 `platform` 模块文件中的 `launch` 目标；`content::guide::publish` 表示更深一层模块中的 `publish` 目标。模块可以理解为项目中的另一份 `.mf` 文件，用来分组保存目标。入口文件 `main.mf` 中的目标只需写本地名称，例如 `start`。文件组织和跨文件引用的实例可以从 [`examples/`](../examples/README.md) 开始查看。

上述 `release` 配方会把文件写到项目目录内的 `dist/release.md`，并用 `templates/release.md.j2` 排版；`guide` 配方会使用内置排版，写到 `dist/guide.md`。这些文件路径以项目根目录为起点。配置中的路径只能指向项目内部，不能是绝对路径，也不能通过上一级路径或符号链接指向项目外部。`mkd` 会检查配置格式、目标名称和路径；只写 `version = 1` 也能得到有效的空配置，但要执行 `mkd build`，还需要配方。

## 常用命令

在上面的配置和项目结构下，可以运行：

```sh
mkd build                         # 生成默认的 release 计划
mkd build guide                   # 生成 guide 计划
mkd build --check                 # 检查目标及配方中指定的模板，不生成文件
mkd build guide -o guide-copy.md  # 临时改用另一个输出文件
mkd build --no-template           # 使用内置 Markdown 排版
mkd target platform::launch -o launch.md  # 不用配置，直接生成计划
mkd target platform::launch --check       # 检查目标，不生成文件
```

`mkd build --template alternate.md.j2` 可在本次构建中改用另一个排版模板；`mkd target start --template alternate.md.j2 -o plan.md` 也可以直接指定模板。模板的写法见[自定义 Markdown 模板](templates.md)。命令行中的 `-o` 和 `--template` 接受绝对路径；写相对路径时，以运行命令时所在的目录为起点。配置文件中的 `output` 和 `template` 则只能写项目内部的相对路径。`--check` 只检查，不创建输出文件或目录；当前配方使用了模板时，还会尝试用实际计划渲染它。

生成计划时，工具会为输出文件创建所需的父目录。为了保护输入内容，输出不能覆盖入口 `.mf`、构建时读取的其他 `.mf`、正在使用的模板或 `mkd.toml`。检查单份文件可以使用 `mkd lint main.mf`，检查项目中的所有 Markfile 可以使用 `mkd lint --all`；格式化文件使用 `mkd fmt main.mf`。这些维护命令不需要构建配方。
