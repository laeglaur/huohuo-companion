# huohuo

English | [中文](#中文)

Live2D desktop companion for macOS. It keeps a selectable Live2D pet on the desktop and provides optional shortcuts for Archive, Folia, and iCity.

This repository does **not** redistribute Live2D models. Download models yourself and follow each model author's license.

## Features

- Always-on-top transparent Tauri desktop companion.
- Auto-discovers Live2D Cubism 4 `.model3.json` models under `local/live2d/`.
- Per-model size persistence.
- Local alpha-bounds cache generated from rendered Live2D pixels.
- Speech/search bubble placed above the character.
- Optional Archive launcher.
- Optional Folia launcher, page search, and notecard bridge.
- iCity launcher.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| `Option + Right` | Open Archive, if configured |
| `Option + Left` | Open Folia, if installed |
| `Option + Up` / `Option + Down` | Switch Live2D model |
| `Option + F` | Search Folia pages |
| `Option + I` | Open iCity |
| `Option + vertical drag on pet` | Resize current model |

`Option + Right` is registered in the Tauri backend as a global shortcut, so the companion window does not need focus.

## Requirements

- macOS
- Node.js and pnpm
- Rust stable toolchain
- Xcode Command Line Tools
- Live2D Cubism 4 models in `.model3.json` format

Install common tools:

```sh
xcode-select --install
corepack enable
rustup update
```

## Live2D Models

Put downloaded model folders under:

```text
local/live2d/
```

Example:

```text
local/live2d/Huohuo/huohuo.model3.json
local/live2d/Mimi/dog.model3.json
```

You can find free Live2D models on sites such as BOOTH:

```text
https://booth.pm/en/search/free%20live2d
```

Model files are ignored by git. Only `local/live2d/.gitkeep` is committed.

## Optional Config

Copy the example config if you need Archive, custom Folia paths, or model folders outside `local/live2d`:

```sh
cp local/config.example.json local/config.json
```

Fields:

```json
{
  "live2dRoots": ["local/live2d"],
  "defaultModelPath": null,
  "archiveAppDir": null,
  "foliaAppPath": null,
  "foliaDataDir": null
}
```

- `live2dRoots`: folders scanned recursively for `.model3.json`.
- `defaultModelPath`: optional first/default model.
- `archiveAppDir`: optional local Archive checkout. If unset, `Option + Right` shows a configuration error.
- `foliaAppPath`: optional Folia app override. If unset, the app checks `/Applications` and `~/Applications`.
- `foliaDataDir`: optional Folia data override. If unset, common Folia data folders are checked.

## Alpha Bounds Cache

On first startup, huohuo queues each discovered model for offline alpha-bounds sampling. The sampler renders the model, samples focus directions around 360 degrees, includes discovered motions/expressions, and writes cache files to:

```text
local/bounds/
```

Runtime reads this local cache on later starts. Cache files are ignored by git because they depend on each user's downloaded models and local paths.

## Install From Source

```sh
git clone git@github.com:laeglaur/huohuo-companion.git
cd huohuo-companion
pnpm install
pnpm build
cd src-tauri && cargo test && cd ..
pnpm exec tauri build --bundles app
```

Built app:

```text
src-tauri/target/release/bundle/macos/huohuo.app
```

Open the app:

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
- `folia-card-requests/`: Folia notecard request files.

## Development Flow

- `dev`: active development branch.
- `main`: stable packaged source. Merge from `dev` when ready to release.

Recommended flow:

```sh
git checkout dev
pnpm build
cd src-tauri && cargo test && cd ..
pnpm exec tauri build --bundles app
git add .
git commit -m "..."
git push origin dev
```

Release flow:

```sh
git checkout main
git merge --ff-only dev
pnpm exec tauri build --bundles app
git push origin main
```

## Notes

- Live2D assets are not included. Each user is responsible for downloading models and checking their licenses.
- Archive is private/user-specific and disabled unless `archiveAppDir` is configured.
- Folia integration is optional. Without Folia, the related shortcuts show a clear unsupported message.
- Packaged apps are intended to be built on the machine where `local/` is prepared.

---

# 中文

macOS Live2D 桌宠应用。它会把可切换的 Live2D 角色固定在桌面上，并提供可选的 Archive、Folia、iCity 快捷入口。

本仓库 **不分发 Live2D 模型素材**。请用户自行下载模型，并遵守每个模型作者的授权说明。

## 功能

- 常驻桌面的透明 Tauri 窗口。
- 自动扫描 `local/live2d/` 下的 Live2D Cubism 4 `.model3.json` 模型。
- 每个模型单独保存大小。
- 根据 Live2D 渲染后的 alpha 像素生成本地透明框缓存。
- 对话/搜索气泡显示在角色上方。
- 可选 Archive 启动入口。
- 可选 Folia 启动、页面搜索和桌面便签桥接。
- iCity 启动入口。

## 快捷键

| 快捷键 | 动作 |
| --- | --- |
| `Option + 右方向键` | 打开 Archive，需要先配置 |
| `Option + 左方向键` | 打开 Folia，需要已安装 |
| `Option + 上/下方向键` | 切换 Live2D 桌宠 |
| `Option + F` | 搜索 Folia page |
| `Option + I` | 打开 iCity |
| `按住 Option 纵向拖拽桌宠` | 调整当前模型大小 |

`Option + 右方向键` 在 Tauri 后端注册为全局快捷键，因此不需要桌宠窗口获得焦点。

## 环境要求

- macOS
- Node.js 和 pnpm
- Rust stable toolchain
- Xcode Command Line Tools
- `.model3.json` 格式的 Live2D Cubism 4 模型

安装常用工具：

```sh
xcode-select --install
corepack enable
rustup update
```

## Live2D 模型

把下载好的模型文件夹放到：

```text
local/live2d/
```

例如：

```text
local/live2d/Huohuo/huohuo.model3.json
local/live2d/Mimi/dog.model3.json
```

可以从 BOOTH 等站点查找免费 Live2D 模型：

```text
https://booth.pm/en/search/free%20live2d
```

模型素材会被 git 忽略。仓库只提交 `local/live2d/.gitkeep` 占位文件。

## 可选配置

如果需要 Archive、自定义 Folia 路径，或者模型不放在 `local/live2d`，复制示例配置：

```sh
cp local/config.example.json local/config.json
```

字段：

```json
{
  "live2dRoots": ["local/live2d"],
  "defaultModelPath": null,
  "archiveAppDir": null,
  "foliaAppPath": null,
  "foliaDataDir": null
}
```

- `live2dRoots`：递归扫描 `.model3.json` 的目录。
- `defaultModelPath`：可选的默认模型。
- `archiveAppDir`：可选的本地 Archive 项目目录。不配置时，`Option + 右方向键` 会提示未配置。
- `foliaAppPath`：可选 Folia app 路径。不配置时，会自动检查 `/Applications` 和 `~/Applications`。
- `foliaDataDir`：可选 Folia 数据目录。不配置时，会自动检查常见 Folia 数据目录。

## 透明框缓存

第一次启动时，huohuo 会把所有已发现模型排队做离线 alpha 透明框采样。采样会渲染模型，覆盖 360 度方向的鼠标注视点，并纳入发现到的 motion/expression，然后写入：

```text
local/bounds/
```

之后启动会直接读取这些本地缓存。缓存文件依赖用户下载的模型和本机路径，所以不会提交到 git。

## 从源码安装

```sh
git clone git@github.com:laeglaur/huohuo-companion.git
cd huohuo-companion
pnpm install
pnpm build
cd src-tauri && cargo test && cd ..
pnpm exec tauri build --bundles app
```

打包结果：

```text
src-tauri/target/release/bundle/macos/huohuo.app
```

打开 app：

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

## 开发流程

- `dev`：日常开发分支。
- `main`：稳定可打包源码。准备发布时从 `dev` 合并。

推荐流程：

```sh
git checkout dev
pnpm build
cd src-tauri && cargo test && cd ..
pnpm exec tauri build --bundles app
git add .
git commit -m "..."
git push origin dev
```

发布流程：

```sh
git checkout main
git merge --ff-only dev
pnpm exec tauri build --bundles app
git push origin main
```

## 说明

- 仓库不包含 Live2D 素材。用户需要自行下载模型，并确认授权允许个人使用。
- Archive 是个人/私有项目，不配置 `archiveAppDir` 时默认禁用。
- Folia 集成是可选的。没有安装 Folia 时，相关快捷键会给出明确提示。
- 打包 app 建议在已经准备好 `local/` 的机器上本地构建。
