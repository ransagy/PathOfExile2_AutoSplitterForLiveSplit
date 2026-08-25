# Path of Exile 2 Auto Splitter (WASM)

A [Path of Exile 2](https://www.pathofexile.com/poe2) auto splitter for
timers built on [LiveSplit One](https://one.livesplit.org)'s sandboxed auto
splitting runtime — i.e. **LiveSplit One**, **livesplit-one-druid**, and OBS's
obs-livesplit-one plugin. Works on Windows, macOS and **Linux**
(including Proton/Wine setups).

This is a Rust/WASM port of the C# LiveSplit component
[`LiveSplit.PathOfExile2AutoSplitter`](../../) by
[SavageOranges](https://github.com/SavageOranges) (MIT licensed, see the
repository root [`LICENSE`](../LICENSE)). Instead of being a Windows-only .NET
component, it tails the game's `Client.txt` just like the original, but runs
inside the cross-platform WASM auto splitting runtime.

## Quick Start

1. Make yourself a folder for speedrun files, e.g. `mkdir ~/livesplit`.
2. Download the prebuilt auto splitter:
   [`poe2_autosplitter.wasm`](./poe2_autosplitter.wasm)
   (committed alongside these sources, mirroring how the original component
   ships its DLL in `Component/DLL/`) — save it into that folder.
3. Grab a splits file from [`splits/`](../splits/) — e.g.
   `Campaign-AnyPercent.lss` — into the same folder.
4. In your timer: **Open Splits** → pick that `.lss`, then right-click →
   **Open Auto Splitter…** → pick `poe2_autosplitter.wasm`.
5. Right-click → **Edit Auto Splitter Settings** → point it at PoE2's
   `Client.txt` and pick your split mode. Save your splits once so the path is
   remembered.

Detailed walkthrough below.

---

## Setup Guide

### 0. Get a suitable timer app

On Linux, use a recent build of
[livesplit-one-druid](https://github.com/AlexKnauth/livesplit-one-druid/releases)
(**0.8 or newer recommended** — older builds lack the settings UI and the TCP
server used for hotkeys). The AUR package `livesplit-one-druid-bin` has been
stale before; downloading the upstream release tarball into `~/.local/bin` is
the safest route:

```sh
tar xzf livesplit-one-<version>-x86_64_v3-unknown-linux-gnu.tar.gz
mv LiveSplitOne ~/.local/bin/livesplit-one
```

### 1. Get the splitter and split files

- Auto splitter: [`poe2_autosplitter.wasm`](./poe2_autosplitter.wasm)
- Splits: pick one from [`splits/`](../splits/)

| File | Contents |
|---|---|
| `Campaign-100.lss` | all campaign areas (96 segments) |
| `Campaign-AnyPercent.lss` | mandatory areas only (78 segments) |
| `Levels-2-100.lss` | one segment per level, 2–100 |

Keep your *active* splits copy somewhere outside any git repository (e.g.
`~/livesplit/`) so the timer saving over them never mixes your personal data
(attempt history, log-file path) with the shipped templates. The templates are
regenerated from upstream sources by [`splits/generate.py`](../splits/generate.py).

### 2. Load everything

1. **Open Splits** → choose your `.lss`.
2. Right-click → **Open Auto Splitter…** → choose `poe2_autosplitter.wasm`.
3. Right-click → **Edit Auto Splitter Settings**:

| Setting | Meaning |
|---|---|
| Client.txt | Path of Exile 2's client log (see below) |
| Splits file | Your `.lss` - used as the trigger set in **Custom** mode |
| Split mode | `Campaign (100%)`, `Campaign (Any%)`, `Level`, or `Custom (from splits file)` |
| Enable auto splitting | master switch for start/split behaviour |
| Auto start new character runs | start automatically when entering The Riverbank |
| Verbose debug logging | per-tick trace lines in `log.txt` (off = quiet; meaningful events always log) |

Pick the split mode that matches your splits file:

- **Campaign (100%)** expects the full 96-area file.
- **Campaign (Any%)** expects the 78 mandatory areas.
- **Custom** fires on exactly the areas whose names appear as segments in the
  splits file you picked under *Splits file* — build any hybrid you like
  (drop optionals, add them back, one label per act boss: non-area labels are
  simply passed over without consuming a segment). Edit the file whenever you
  want; the splitter notices changes on its own. The run ends when you enter
  the **last segment** of the file, so put your finish-line zone there.

#### How triggering works

- A split fires on the **first entry** of each tracked area per attempt.
  Everything else — checkpoint resets, deaths, instance recreations,
  waypoint/backtrack revisits, pause/resume — is silent and never consumes a
  segment. Mid-run game restarts are survived cleanly.
- Each zone entry completes the **previous** unconsumed row. So for accurate
  labels, keep Custom-mode segment order identical to your planned
  first-entry order. Freeform label rows (e.g. `Act 1 Boss`) are passed over
  without consuming anything.
- Entering the finish line fires a double-split: the previous row *and* the
  finish row get stamped at the same instant — seeing the last two segments
  with identical times is expected.
- The Riverbank (run start) never consumes a segment, whether you use auto
  start or start manually.

4. **Save your splits once** (Splits → Save). The auto splitter's settings are
   stored inside the `.lss`, so this makes them permanent. They are restored
   automatically every time you reopen that file — no reconfiguration needed,
   ever. Settings are per-splits-file: give each file its own save after first
   configuration.

### 3. Where Client.txt lives

- **Windows (Steam):**
  `C:\Program Files (x86)\Steam\steamapps\common\Path of Exile 2\logs\Client.txt`
- **Linux (native Steam install, second library drive):**
  `<library>/steamapps/common/Path of Exile 2/logs/Client.txt`
- **Linux (Steam Proton, appid 2694490):**
  `~/.steam/steam/steamapps/compatdata/2694490/pfx/drive_c/users/steamuser/Documents/My Games/Path of Exile 2/client.txt`
- **Wine/Lutris:** same relative path inside that wine prefix.

The sandbox maps the host filesystem under `/mnt`; the timer converts your
picked path automatically.

### 4. Hotkeys on Linux/Wayland (server trick)

Global hotkeys of livesplit-one-druid rely on X11 grabs, which do not receive
keys under Wayland sessions; the evdev fallback requires being in the `input`
group. The reliable alternative uses the timer's built-in TCP server plus
desktop-level custom shortcuts — fully Wayland-proof:

1. Enable the server: right-click → **Settings** → Server → **Enable Server**,
   port `16834` → OK. (Or add to `~/.local/share/livesplitone/config.yml`:)

   ```yaml
   server:
     enable: true
     port: 16834
   ```

2. **Restart the timer** — the server starts at launch.
3. Install the helper script from this directory:

   ```sh
   cp lss ~/.local/bin/ && chmod +x ~/.local/bin/lss
   ```

4. Bind desktop custom shortcuts (KDE: System Settings → Keyboard → Shortcuts
   → Add Command) to commands like:

   | Action | Command |
   |---|---|
   | Start run / advance | `lss splitOrStart` |
   | Split | `lss split` |
   | Reset | `lss reset` |
   | Undo split | `lss undoSplit` |
   | Skip split | `lss skipSplit` |
   | Pause toggle | `lss togglePauseOrStart` |
   | Test connection | `lss ping` |

These work whether or not the timer window is focused, on any compositor.
The server speaks JSON (`{"command":"SplitOrStart"}`) and also accepts classic
plaintext LiveSplit server commands (`split`), if you prefer raw `nc`/curl.

### 5. Verify & troubleshoot

Enable logging in `~/.local/share/livesplitone/config.yml`:

```yaml
log:
  enable: true
  level: INFO
  clear: true
```

Restart, then check `~/.local/share/livesplitone/log.txt`:

- `Loaded auto splitter` + `Watching Client.txt` — splitter running and the
  log file opened successfully.
- `[dbg] +N new log lines` — new lines are being consumed.
- `[dbg] split: <area>` / `[dbg] level N` / `[dbg] start (The Riverbank)` —
  events firing.
- `[dbg] unknown area id: …` — the game logged an area we don't track (please
  report!).
- Nothing at all? Zone transitions only emit lines on **loading screens**;
  walking around inside a zone produces nothing. Also make sure the zones you
  enter come *after* loading the auto splitter.

Known quirks:

- Saving splits through the timer rewrites the file in its own compact style
  and currently drops segment icons (cosmetic; re-run
  `python3 splits/generate.py` to get pristine templates back).
- The final two segments of a finished run share an identical time (the
  finish-line double-split stamps both at the same zone load) - expected.
- Cemetery of the Eternals timing is approximate in the stock campaign files
  (Lachlann revisit quirk, inherited from upstream). In Custom mode you can
  collapse the whole cemetery questline into a single segment instead.
- No load-time removal yet (upstream TODO as well).

## Building from source

```sh
rustup target add wasm32-wasip1
cargo build --release --target wasm32-wasip1
# output: target/wasm32-wasip1/release/poe2_autosplitter.wasm
```

The committed `poe2_autosplitter.wasm` is refreshed manually whenever the
source changes (same approach as upstream committing its built DLL).

## Credits & license

- Original C# component & campaign data:
  [SavageOranges/PathOfExile2_AutoSplitterForLiveSplit](https://github.com/SavageOranges/PathOfExile2_AutoSplitterForLiveSplit)
  © SavageOranges, MIT License.
- Built with [asr](https://github.com/LiveSplit/asr) /
  [livesplit-core](https://github.com/LiveSplit/livesplit-core).
- This port: MIT License (see root LICENSE).
