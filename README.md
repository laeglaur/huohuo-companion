# huohuo

huohuo 是一个 macOS Live2D 桌宠。它会把你自己下载的 Live2D 模型放在桌面上，支持切换角色、调整每个角色的大小，并提供可选的 Archive、Folia、iCity 快捷入口。

<video src="assets/demo.mp4" controls width="360"></video>

> 本仓库不包含也不再分发任何 Live2D 模型素材。用户需要自行下载模型，并遵守每个模型作者的授权条款。

## 你可以用它做什么

- 在 macOS 桌面上显示一个常驻、透明背景的 Live2D 桌宠。
- 把多个 Live2D Cubism 4 `.model3.json` 模型放进 `local/live2d/` 后自动识别。
- 用快捷键切换桌宠，并为每个桌宠单独记住大小。
- 第一次导入模型时手动入库透明框，避免尾巴、鼻子、漂浮物、附件被裁掉。
- 可选打开本地 Archive、Folia，并用 Folia 搜索/便签桥接。
- 可选打开 iCity 网页。

## 快速开始

### 1. 准备环境

需要 macOS、Node.js/pnpm、Rust stable 和 Xcode Command Line Tools。

```sh
xcode-select --install
corepack enable
rustup update
```

### 2. 下载并构建

```sh
git clone git@github.com:laeglaur/huohuo-companion.git
cd huohuo-companion
pnpm install
pnpm build
cd src-tauri && cargo test && cd ..
pnpm exec tauri build --bundles app
```

打包后的 app 在：

```text
src-tauri/target/release/bundle/macos/huohuo.app
```

打开：

```sh
open -a "$PWD/src-tauri/target/release/bundle/macos/huohuo.app"
```

### 3. 放入 Live2D 模型

把你下载好的 Live2D 模型文件夹放到：

```text
local/live2d/
```

示例：

```text
local/live2d/Huohuo/huohuo.model3.json
local/live2d/Mimi/dog.model3.json
```

重启 huohuo 后，它会递归扫描 `local/live2d/` 下的 `.model3.json`。

## 快捷键

| 快捷键 | 动作 |
| --- | --- |
| `Option + 上/下方向键` | 切换 Live2D 桌宠 |
| `按住 Option 纵向拖拽桌宠` | 调整当前桌宠大小 |
| `Option + B` | 重新入库当前桌宠透明框 |
| `Option + 右方向键` | 打开 Archive，需要配置 |
| `Option + 左方向键` | 打开 Folia，需要已安装 |
| `Option + F` | 搜索 Folia page |
| `Option + I` | 打开 iCity |

`Option + 右方向键` 在 Tauri 后端注册为全局快捷键，所以桌宠窗口不需要获得焦点。

## Live2D 模型下载建议

你可以从 BOOTH 等网站搜索免费或可个人使用的 Live2D 模型：

```text
https://booth.pm/en/search/free%20live2d
```

本项目开发时测试过以下模型或搜索关键词。它们只是兼容性参考，不随仓库分发：

| 模型/关键词 | 备注 |
| --- | --- |
| `huohuo` / `藿藿 Live2D` | fan model。请特别确认作者说明、二创规则、是否允许直播/商用/再分发。 |
| `Green Junimo by Eisrynn` | README 中要求 credit，且禁止再分发模型。 |
| `Jack in the box` | VTube Studio / Live2D 模型关键词。 |
| `Mimi dog` / `dog.model3.json` | 小狗模型关键词。 |
| `Nagito vtuber model` | 本地测试中自动去重，避免同一模型导入两次。 |
| `Scuffed Neko` | 猫猫模型关键词。 |
| `Cecilia_V4` / `Cecilia live2d` | Cecilia 模型关键词。 |
| `piaopiao` / `버츄얼 슬라임 ver2` | slime 模型关键词。 |
| `011chasham`、`018haibuchi`、`023doro` | 测试过的猫猫头模型名。 |

下载模型后，请把整个模型文件夹放入 `local/live2d/`。不要把模型素材提交到 git；本仓库已经忽略 `local/**`。

## 第一次导入：透明框入库

Live2D 的实际可见区域常常比模型文件声明的区域小很多，也可能因为动作、表情、尾巴、鼻子或漂浮物变大。huohuo 用“透明框入库”记录每个模型的实际可点击/显示范围。

当某个模型没有本地透明框缓存时，huohuo 会显示入库面板：

- 移动鼠标，让 Live2D 看向不同方向，尽量覆盖一圈。
- 绿色实线框是当前正在采样的范围。
- 面板里的候选按钮是已经保存过的框；选中候选时，画面上只显示这个候选的虚线框。
- `确认`：保存当前状态的框。
- `继承选中`：当前状态大小差不多时，复用已选候选框。
- `重扫`：清空当前状态重新扫。
- `稍后`：停止入库，保留已经完成的缓存。

入库缓存写入：

```text
local/bounds/
```

这些缓存依赖每个用户的模型文件和本机路径，不会提交到 git。

## 可选配置

如果需要 Archive、自定义 Folia 路径，或者想扫描 `local/live2d/` 之外的模型目录，复制示例配置：

```sh
cp local/config.example.json local/config.json
```

配置格式：

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
- `defaultModelPath`：可选默认模型路径。
- `archiveAppDir`：可选本地 Archive 项目目录。不配置时，`Option + 右方向键` 会提示未配置。
- `foliaAppPath`：可选 Folia app 路径。不配置时，会检查 `/Applications` 和 `~/Applications`。
- `foliaDataDir`：可选 Folia 数据目录。不配置时，会检查常见 Folia 数据目录。

## 运行数据

huohuo 的设置和日志保存在：

```text
~/Library/Application Support/com.laeglaur.huohuo-companion
```

常见文件：

- `settings.json`：当前模型、窗口位置、每个模型的大小、上次 Archive 端口。
- `companion.log`：前后端事件日志。
- `folia-card-requests/`：Folia 便签请求文件。

## 开发流程

- `dev`：日常开发分支。
- `main`：稳定可打包源码。准备发布时从 `dev` 合并。

常用开发命令：

```sh
git checkout dev
pnpm build
cd src-tauri && cargo test && cd ..
pnpm exec tauri build --bundles app
```

发布流程：

```sh
git checkout main
git merge --ff-only dev
pnpm exec tauri build --bundles app
git push origin main
```

## 说明

- 本仓库不包含 Live2D 模型素材。用户需要自行下载，并确认授权允许自己的使用方式。
- Archive 是个人/私有项目，不配置 `archiveAppDir` 时默认不可用。
- Folia 集成是可选的。没有安装 Folia 时，相关快捷键会给出提示。
- 打包 app 建议在已经准备好 `local/` 的机器上本地构建。

---

# English

huohuo is a macOS Live2D desktop companion. It keeps user-provided Live2D Cubism 4 models on the desktop, supports model switching and per-model size persistence, and optionally opens Archive, Folia, and iCity.

This repository does **not** redistribute Live2D models. Download models yourself and follow each model author's license.

## Quick Start

```sh
git clone git@github.com:laeglaur/huohuo-companion.git
cd huohuo-companion
pnpm install
pnpm build
cd src-tauri && cargo test && cd ..
pnpm exec tauri build --bundles app
open -a "$PWD/src-tauri/target/release/bundle/macos/huohuo.app"
```

Put downloaded Live2D model folders under:

```text
local/live2d/
```

Optional local config:

```sh
cp local/config.example.json local/config.json
```

## Shortcuts

| Shortcut | Action |
| --- | --- |
| `Option + Up/Down` | Switch Live2D model |
| `Option + vertical drag on pet` | Resize current model |
| `Option + B` | Re-run bounds onboarding for the current model |
| `Option + Right` | Open Archive, if configured |
| `Option + Left` | Open Folia, if installed |
| `Option + F` | Search Folia pages |
| `Option + I` | Open iCity |

## Model Notes

The app auto-discovers `.model3.json` files under `local/live2d/`. Models tested during development include Huohuo, Green Junimo by Eisrynn, Jack in the box, Mimi dog, Nagito vtuber model, Scuffed Neko, Cecilia_V4, piaopiao / virtual slime ver2, and 011chasham / 018haibuchi / 023doro.

These names are compatibility references only. Assets are not included and must not be redistributed through this repository.
