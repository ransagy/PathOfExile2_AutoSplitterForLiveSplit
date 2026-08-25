# Path of Exile 2 Auto Splitter (WASM)

A [Path of Exile 2](https://www.pathofexile.com/poe2) auto splitter for
timers built on [LiveSplit One](https://one.livesplit.org)'s sandboxed auto
splitting runtime — i.e. **LiveSplit One**, **livesplit-one-druid**, and OBS's
obs-livesplit-one plugin. Works on Windows, macOS and **Linux**
(including Proton/Wine setups).

## Quick Start

1. Download the prebuilt auto splitter:
   [`wasm/poe2_autosplitter.wasm`](./poe2_autosplitter.wasm)
   (committed alongside these sources, mirroring how the original component
   ships its DLL in `Component/DLL/`).
2. Grab a splits file from [`splits/`](../splits/) — e.g.
   `Campaign-AnyPercent.lss`.
3. In your timer: **Open Splits** → pick that `.lss`, then right-click →
   **Open Auto Splitter…** → pick `poe2_autosplitter.wasm`.
4. Right-click → **Edit Auto Splitter Settings** → point it at PoE2's
   `Client.txt` and pick your split mode. Save your splits once so the path is
   remembered.

This is a Rust/WASM port of the C# LiveSplit component
[`LiveSplit.PathOfExile2AutoSplitter`](../../) by
[SavageOranges](https://github.com/SavageOranges) (MIT licensed, see the
repository root [`LICENSE`](../LICENSE)). Instead of being a Windows-only .NET
component, it tails the game's `Client.txt` just like the original, but runs
inside the cross-platform WASM auto splitting runtime.

## Features

Same split modes as the original component:

- **Campaign (100%)** — splits on first entry of every tracked campaign area.
- **Campaign (Any%)** — splits on first entry of mandatory areas only
  (including Ascendancy trials).
- **Level** — splits on every newly reached level.
- **Auto start** — starts the timer when entering The Riverbank on a fresh
  character.
- Entering **The Ziggurat Refuge** fires an additional split to end the run,
  mirroring the original component's behaviour.

Settings are exposed through the timer's Auto Splitter Settings dialog:

| Setting | Description |
|---|---|
| Client.txt | Path to PoE2's `Client.txt` log file |
| Split mode | Campaign (100%) / Campaign (Any%) / Level |
| Enable auto splitting | Master switch for start/split behaviour |
| Auto start new character runs | Start automatically at The Riverbank |

## Building

The committed `poe2_autosplitter.wasm` is refreshed manually whenever the
source changes (same approach as upstream committing its built DLL). To build
it yourself:

```sh
rustup target add wasm32-wasip1
cargo build --release --target wasm32-wasip1
# output: target/wasm32-wasip1/release/poe2_autosplitter.wasm
```

## Usage (livesplit-one-druid)

Ready-made split files mirroring the original component's "Generate" output
(same area order, same icons) live in [`splits/`](../splits/):

- `Campaign-100.lss` — all campaign areas (96 segments)
- `Campaign-AnyPercent.lss` — mandatory areas only (78 segments)
- `Levels-2-100.lss` — one segment per level, 2–100

1. **Open Splits** → pick one of the files above.
2. Right-click → **Open Auto Splitter…** → select the built `.wasm`.
3. Right-click → **Edit Auto Splitter Settings** → point it at `Client.txt`
   and set the matching split mode.
4. CLI shortcut:
   `livesplit-one splits/Campaign-AnyPercent.lss -a wasm/target/wasm32-wasip1/release/poe2_autosplitter.wasm`

To regenerate the split files after syncing upstream changes:

```sh
python3 splits/generate.py
```

### Where Client.txt lives

- **Windows (Steam):** `C:\Program Files (x86)\Steam\steamapps\common\Path of Exile 2\logs\Client.txt`
- **Linux (Steam Proton, appid 2694490):**
  `~/.steam/steam/steamapps/compatdata/2694490/pfx/drive_c/users/steamuser/Documents/My Games/Path of Exile 2/client.txt`
- **Wine/Lutris:** same relative path inside the wine prefix.

Note: the sandbox maps the host filesystem under `/mnt`; the timer converts
your picked path automatically.

## Differences from the C# component

- Split mode is a preset choice instead of individually checkboxed areas.
- Level mode splits on every new level (equivalent to generating all levels).
- No load-time removal yet (upstream TODO as well).
- No automatic splits-file generation with icons; use any PoE2 `.lss`.

## Troubleshooting

Enable logging in the livesplit-one-druid config
(`~/.local/share/livesplitone/config.yml`):

```yaml
log:
  enable: true
  level: INFO
  clear: true
```

A `log.txt` appears next to the config; splitter messages (and errors) land
there. The splitter prints `Watching Client.txt` once the file opens
successfully.

## Credits & license

- Original C# component & campaign data:
  [SavageOranges/PathOfExile2_AutoSplitterForLiveSplit](https://github.com/SavageOranges/PathOfExile2_AutoSplitterForLiveSplit)
  © SavageOranges, MIT License.
- Built with [asr](https://github.com/LiveSplit/asr) /
  [livesplit-core](https://github.com/LiveSplit/livesplit-core).
- This port: MIT License (see root LICENSE).
