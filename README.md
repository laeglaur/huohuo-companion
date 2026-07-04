# huohuo

Live2D desktop companion for quickly opening Archive, Folia, and iCity on macOS.

## What It Does

- Keeps a Live2D character on the desktop as an always-on-top transparent Tauri window.
- Discovers selectable Live2D models from:
  - `/Users/laeglaur/Documents/code/record/huohuo`
  - `/Users/laeglaur/Documents/code/record/anime`
- Opens Archive without manually starting a server or visiting a browser URL.
- Opens Folia and Folia page cards from the desktop companion.
- Opens iCity from the desktop companion.
- Stores per-model scale and position settings locally.
- Uses alpha bounds from rendered Live2D frames to keep the transparent click area close to the visible model.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| `Option + Right` | Open Archive |
| `Option + Left` | Open Folia |
| `Option + Up` / `Option + Down` | Switch Live2D model |
| `Option + F` | Open Folia search |
| `Option + I` | Open iCity |
| `Option + vertical drag on pet` | Resize the current model |

`Option + Right` is registered as a backend global shortcut, so it does not require the companion window to have keyboard focus.

## Local Paths

This app currently uses local absolute paths:

- Archive app: `/Users/laeglaur/Documents/code/record/archive_app`
- Default Huohuo model: `/Users/laeglaur/Documents/code/record/huohuo/huohuo.model3.json`
- Extra Live2D models: `/Users/laeglaur/Documents/code/record/anime`
- Folia app: `/Users/laeglaur/Documents/code/notebook/src-tauri/target/release/bundle/macos/folia.app`
- Folia data: `/Users/laeglaur/Library/Application Support/com.laeglaur.notebook`

## Development

Install dependencies:

```sh
pnpm install
```

Run the Tauri development app:

```sh
pnpm tauri:dev
```

Build the frontend:

```sh
pnpm build
```

Run Rust tests:

```sh
cd src-tauri
cargo test
```

Build the macOS app bundle:

```sh
pnpm exec tauri build --bundles app
```

The built app is written to:

```text
src-tauri/target/release/bundle/macos/huohuo.app
```

## Runtime Data

Settings and logs are stored under:

```text
~/Library/Application Support/com.laeglaur.huohuo-companion
```

Important files:

- `settings.json`: selected model, window position, per-model scale, last Archive port.
- `companion.log`: frontend/backend event log.
- `folia-card-requests/`: Folia card request files.

Live2D alpha bounds are cached in webview `localStorage` using `huohuo.modelBounds.v3`.

## Notes

- `Option + Right` opens Archive through the Tauri backend and creates an Archive webview window.
- The companion window reserves a small top area for the speech bubble so the bubble does not cover the Live2D face.
- The dog model has an expanded alpha bounds margin for its long nose; this affects the transparent frame, not its scale.
- The project is currently tuned for this machine's local folder layout.
