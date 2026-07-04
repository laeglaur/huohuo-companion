# huohuo

English | [中文](#中文)

Live2D desktop companion for macOS. It keeps a selectable Live2D pet on the desktop and provides quick shortcuts for Archive, Folia, and iCity.

> Current status: this is a personal desktop app with local absolute paths. Other users can build and run it, but they must prepare the dependent apps and update the paths documented below.

## Features

- Always-on-top transparent Tauri desktop companion.
- Live2D model discovery from a default Huohuo folder and an extra anime model folder.
- Per-model size persistence.
- Tight transparent window bounds based on rendered Live2D alpha pixels.
- Speech bubble area reserved above the character so the bubble does not cover the face.
- Archive launcher with a backend global shortcut.
- Folia launcher and Folia page search/card bridge.
- iCity launcher.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| `Option + Right` | Open Archive |
| `Option + Left` | Open Folia |
| `Option + Up` / `Option + Down` | Switch Live2D model |
| `Option + F` | Open Folia search |
| `Option + I` | Open iCity |
| `Option + vertical drag on pet` | Resize current model |

`Option + Right` is registered in the Tauri backend as a global shortcut, so it does not require the companion window to be focused.

## Requirements

- macOS
- Node.js and pnpm
- Rust stable toolchain
- Xcode Command Line Tools
- A local Archive checkout
- A local Folia app bundle
- Live2D Cubism 4 models in `.model3.json` format

Install common tools:

```sh
xcode-select --install
corepack enable
rustup update
```

## Required Local Paths

Before building for another machine, update these constants in `src-tauri/src/lib.rs` and `src/main.ts`.

Current defaults:

```text
Archive app:
/Users/laeglaur/Documents/code/record/archive_app

Default Huohuo model:
/Users/laeglaur/Documents/code/record/huohuo/huohuo.model3.json

Extra Live2D model folder:
/Users/laeglaur/Documents/code/record/anime

Folia app:
/Users/laeglaur/Documents/code/notebook/src-tauri/target/release/bundle/macos/folia.app

Folia data:
/Users/laeglaur/Library/Application Support/com.laeglaur.notebook
```

Files to edit:

- `src-tauri/src/lib.rs`
- `src/main.ts`
- `src-tauri/tauri.conf.json` asset protocol scope, if your Live2D models live outside the current `/Users/laeglaur/Documents/code/record/**` scope.

## Install From Source

Clone the repo:

```sh
git clone git@github.com:laeglaur/huohuo-companion.git
cd huohuo-companion
```

Install JavaScript dependencies:

```sh
pnpm install
```

Build the frontend:

```sh
pnpm build
```

Run Rust tests:

```sh
cd src-tauri
cargo test
cd ..
```

Run in development:

```sh
pnpm tauri:dev
```

Build the macOS app bundle:

```sh
pnpm exec tauri build --bundles app
```

Built app:

```text
src-tauri/target/release/bundle/macos/huohuo.app
```

Open the built app:

```sh
open -n -a "$PWD/src-tauri/target/release/bundle/macos/huohuo.app"
```

## Runtime Data

Settings and logs are stored at:

```text
~/Library/Application Support/com.laeglaur.huohuo-companion
```

Important files:

- `settings.json`: selected model, window position, per-model scale, last Archive port.
- `companion.log`: frontend/backend event log.
- `folia-card-requests/`: Folia card request files.

Live2D alpha bounds are cached in the webview localStorage under:

```text
huohuo.modelBounds.v3
```

## Development Flow

- `main`: stable packaged source. Releases are built from this branch.
- `dev`: active development branch.

Recommended workflow:

```sh
git checkout dev
# make changes
pnpm build
cd src-tauri && cargo test && cd ..
pnpm exec tauri build --bundles app
git commit -am "..."
git push origin dev
```

When ready to package:

```sh
git checkout main
git merge --ff-only dev
pnpm exec tauri build --bundles app
git push origin main
```

## Known Limitations

- The app currently uses hard-coded local paths and is not a generic installer.
- Archive and Folia must already exist locally.
- Folia database search assumes the current Folia/Notebook data directory layout.
- Some Live2D models may need per-model bounds tuning when their transparent canvas is much larger than the visible character.

---

# 中文

macOS Live2D 桌宠应用。它会把可切换的 Live2D 角色固定在桌面上，并提供 Archive、Folia、iCity 的快捷入口。

> 当前状态：这是一个个人自用桌面应用，代码里包含本机绝对路径。其他人可以从源码构建运行，但必须先准备依赖应用，并按下面说明修改路径。

## 功能

- 常驻桌面的透明 Tauri 窗口。
- 从默认 Huohuo 文件夹和 anime 文件夹发现 Live2D 模型。
- 每个模型单独保存大小。
- 根据 Live2D 渲染后的 alpha 像素计算透明窗口边界。
- 顶部预留气泡区域，避免气泡挡住角色脸。
- Archive 快捷启动，并支持后端全局快捷键。
- Folia 启动、Folia 页面搜索和桌面便签桥接。
- iCity 启动。

## 快捷键

| 快捷键 | 动作 |
| --- | --- |
| `Option + 右方向键` | 打开 Archive |
| `Option + 左方向键` | 打开 Folia |
| `Option + 上/下方向键` | 切换 Live2D 桌宠 |
| `Option + F` | 打开 Folia 搜索 |
| `Option + I` | 打开 iCity |
| `按住 Option 纵向拖拽桌宠` | 调整当前模型大小 |

`Option + 右方向键` 已在 Tauri 后端注册为全局快捷键，因此不需要桌宠窗口获得键盘焦点。

## 环境要求

- macOS
- Node.js 和 pnpm
- Rust stable toolchain
- Xcode Command Line Tools
- 本地 Archive 项目
- 本地 Folia app bundle
- `.model3.json` 格式的 Live2D Cubism 4 模型

安装常用工具：

```sh
xcode-select --install
corepack enable
rustup update
```

## 必须修改的本地路径

如果要在另一台机器上构建，请先修改 `src-tauri/src/lib.rs` 和 `src/main.ts` 里的路径常量。

当前默认值：

```text
Archive app:
/Users/laeglaur/Documents/code/record/archive_app

默认 Huohuo 模型:
/Users/laeglaur/Documents/code/record/huohuo/huohuo.model3.json

额外 Live2D 模型目录:
/Users/laeglaur/Documents/code/record/anime

Folia app:
/Users/laeglaur/Documents/code/notebook/src-tauri/target/release/bundle/macos/folia.app

Folia 数据目录:
/Users/laeglaur/Library/Application Support/com.laeglaur.notebook
```

需要检查的文件：

- `src-tauri/src/lib.rs`
- `src/main.ts`
- `src-tauri/tauri.conf.json` 里的 asset protocol scope。如果你的 Live2D 模型不在 `/Users/laeglaur/Documents/code/record/**` 下面，也要改这里。

## 从源码安装

克隆仓库：

```sh
git clone git@github.com:laeglaur/huohuo-companion.git
cd huohuo-companion
```

安装 JavaScript 依赖：

```sh
pnpm install
```

构建前端：

```sh
pnpm build
```

运行 Rust 测试：

```sh
cd src-tauri
cargo test
cd ..
```

开发模式运行：

```sh
pnpm tauri:dev
```

打包 macOS app：

```sh
pnpm exec tauri build --bundles app
```

打包结果：

```text
src-tauri/target/release/bundle/macos/huohuo.app
```

打开打包后的 app：

```sh
open -n -a "$PWD/src-tauri/target/release/bundle/macos/huohuo.app"
```

## 运行数据

设置和日志保存在：

```text
~/Library/Application Support/com.laeglaur.huohuo-companion
```

重要文件：

- `settings.json`：当前模型、窗口位置、每个模型的大小、上次 Archive 端口。
- `companion.log`：前后端事件日志。
- `folia-card-requests/`：Folia 便签请求文件。

Live2D alpha 边界缓存在 webview localStorage：

```text
huohuo.modelBounds.v3
```

## 开发流程

- `main`：稳定可打包源码。发布从这个分支构建。
- `dev`：日常开发分支。

推荐流程：

```sh
git checkout dev
# 修改代码
pnpm build
cd src-tauri && cargo test && cd ..
pnpm exec tauri build --bundles app
git commit -am "..."
git push origin dev
```

准备发布时：

```sh
git checkout main
git merge --ff-only dev
pnpm exec tauri build --bundles app
git push origin main
```

## 当前限制

- 当前版本包含硬编码本地路径，还不是通用安装器。
- Archive 和 Folia 需要先在本机准备好。
- Folia 搜索依赖当前 Folia/Notebook 的数据目录结构。
- 有些 Live2D 模型的透明画布远大于角色本体，可能需要单独调 alpha 边界。
