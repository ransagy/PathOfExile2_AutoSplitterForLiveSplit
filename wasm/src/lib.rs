//! Path of Exile 2 auto splitter for LiveSplit One's sandboxed auto splitting
//! runtime.
//!
//! This is a Rust/WASM port of the C# LiveSplit component
//! `LiveSplit.PathOfExile2AutoSplitter`
//! (<https://github.com/SavageOranges/PathOfExile2_AutoSplitterForLiveSplit>),
//! which is © SavageOranges and contributors, MIT licensed. The port tails the
//! game's `Client.txt` log file instead of attaching to the process, so it also
//! works on Linux/macOS where the game runs through Proton/Wine.
//!
//! Splitting behaviour mirrors the original component:
//! - **Campaign (100%)**: split on first entry of every campaign area.
//! - **Campaign (Any%)**: split on first entry of mandatory areas only
//!   (including Ascendancy trials).
//! - **Level**: split on every newly reached level.
//! - **Auto start**: starts the timer when entering The Riverbank.
//! - Entering **The Ziggurat Refuge** fires an additional split to end the run.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::time::{Duration, Instant};

use asr::{
    future::next_tick,
    settings::gui::{FileSelect, Title},
    settings::Gui,
    timer,
    timer::TimerState,
};

asr::async_main!(stable);

// ---------------------------------------------------------------------------
// Campaign data (ported from Component/Constants/CampaignAreas.cs)
// ---------------------------------------------------------------------------

const START_AREA_ID: &str = "G1_1";
const END_AREA_ID: &str = "G_Endgame_Town";

struct Area {
    /// Area ID as emitted by the game's log (`Generating level N area "<id>"`).
    ///
    /// Note: the log carries IDs, not display names - same as the original C#
    /// component, which looks areas up by AreaId.
    id: &'static str,
    /// Display name (matches segment names in the shipped split files).
    name: &'static str,
    /// Whether the area is part of the mandatory (Any%) route.
    mandatory: bool,
}

macro_rules! areas {
    ($($id:literal, $name:literal, $mandatory:literal);* $(;)?) => {
        &[ $( Area { id: $id, name: $name, mandatory: $mandatory } ),* ]
    };
}

/// All tracked campaign areas. Order does not matter; splitting is driven by
/// the order areas are entered in-game (same linear behaviour as the original
/// component).
static AREAS: &[Area] = areas! {
    // Act 1
    "G1_1", "The Riverbank", true;
    "G1_town", "Clearfell Encampment", true;
    "G1_2", "Clearfell", true;
    "G1_3", "Mud Burrow", false;
    "G1_4", "The Grelwood", true;
    "G1_5", "The Red Vale", true;
    "G1_6", "The Grim Tangle", true;
    "G1_7", "Cemetery of the Eternals", true;
    "G1_8", "Mausoleum of the Praetor", true;
    "G1_9", "Tomb of the Consort", true;
    "G1_11", "Hunting Grounds", true;
    "G1_12", "Freythorn", false;
    "G1_13_1", "Ogham Farmlands", true;
    "G1_13_2", "Ogham Village", true;
    "G1_14", "The Manor Ramparts", true;
    "G1_15", "Ogham Manor", true;
    // Act 2
    "G2_1", "Vastiri Outskirts", true;
    "G2_town", "The Ardura Caravan", true;
    "G2_10_1", "Mawdun Quarry", true;
    "G2_10_2", "Mawdun Mine", true;
    "G2_2", "Traitor's Passage", true;
    "G2_3a", "The Halani Gates (blocked)", true;
    "G2_3", "The Halani Gates", true;
    "G2_4_1", "Keth", true;
    "G2_4_2", "The Lost City", true;
    "G2_4_3", "Buried Shrines", true;
    "G2_5_1", "Mastodon Badlands", true;
    "Abyss_Intro", "Lightless Passage", false;
    "Abyss_Hub", "The Well of Souls", false;
    "G2_5_2", "The Bone Pits", true;
    "G2_6", "Valley of the Titans", true;
    "G2_7", "The Titan Grotto", true;
    "G2_8", "Deshar", true;
    "G2_9_1", "Path of Mourning", true;
    "G2_9_2", "The Spires of Deshar", true;
    "G2_12", "Dreadnought", true;
    "G2_13", "Trial of the Sekhemas", false;
    // Act 3
    "G3_1", "Sandswept Marsh", true;
    "G3_town", "Ziggurat Encampment", true;
    "G3_3", "Jungle Ruins", true;
    "G3_4", "The Venom Crypts", false;
    "G3_2_1", "Infested Barrens", true;
    "G3_7", "The Azak Bog", false;
    "G3_5", "Chimeral Wetlands", true;
    "G3_6_1", "Jiquani's Machinarium", true;
    "G3_6_2", "Jiquani's Sanctum", true;
    "G3_2_2", "The Matlan Waterways", true;
    "G3_8", "The Drowned City", true;
    "G3_9", "The Molten Vault", false;
    "G3_11", "Apex of Filth", true;
    "G3_12", "Temple of Kopec", true;
    "G3_14", "Utzaal", true;
    "G3_16", "Aggorat", true;
    "G3_17", "The Black Chambers", true;
    "G3_10_Airlock", "The Temple of Chaos", false;
    // Act 4
    "G4_town", "Kingsmarch", true;
    "G4_1_1", "Isle of Kin", true;
    "G4_1_2", "Volcanic Warrens", true;
    "G4_4_1", "Eye of Hinekora", false;
    "G4_4_2", "Halls of the Dead", false;
    "G4_4_3", "Trial of the Ancestors", false;
    "G4_2_1", "Kedge Bay", false;
    "G4_2_2", "Journey's End", false;
    "G4_5_1", "Abandoned Prison", true;
    "G4_5_2", "Solitary Confinement", true;
    "G4_3_1", "Whakapanu Island", false;
    "G4_3_2", "Singing Caverns", false;
    "G4_7", "Shrike Island", true;
    "G4_8b", "Arastas", true;
    "G4_10", "The Excavation", true;
    "G4_11_1b", "Ngakanu", true;
    "G4_11_2", "Heart of the Tribe", true;
    "G4_13", "Plunder's Point", false;
    // Interlude 1
    "P1_Town", "The Refuge", true;
    "P1_1", "Scorched Farmlands", true;
    "P1_2", "Stones of Serle", true;
    "P1_3", "The Blackwood", true;
    "P1_4", "Holten", true;
    "P1_5", "Wolvenhold", true;
    "P1_6", "Holten Estate", true;
    // Interlude 2
    "P2_Town", "The Khari Bazaar", true;
    "P2_1", "The Khari Crossing", true;
    "P2_2", "Pools of Khatal", true;
    "P2_3", "Sel Khari Sanctuary", true;
    "P2_5", "The Galai Gates", true;
    "P2_6", "Qimah", true;
    "P2_7", "Qimah Reservoir", true;
    // Interlude 3
    "P3_Town", "The Glade", true;
    "P3_1", "Ashen Forest", true;
    "P3_2", "Kriar Village", true;
    "P3_3", "Glacial Tarn", true;
    "P3_4", "Howling Caves", false;
    "P3_5", "Kriar Peaks", true;
    "P3_6", "Etched Ravine", true;
    "P3_7", "The Cuachic Vault", true;
    // Endgame
    "G_Endgame_Town", "The Ziggurat Refuge", true;
};

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Gui, Clone, Copy, PartialEq, Eq)]
enum SplitMode {
    /// Campaign (100%) - split on every campaign area
    #[default]
    Full,
    /// Campaign (Any%) - split on mandatory areas only
    AnyPercent,
    /// Level - split on every level up
    Level,
    /// Custom - split on areas present in your splits file
    Custom,
}

/// Which zone a segment's time is attributed to: the one you left (Exit)
/// or the one you entered (Entry).
#[derive(Gui, Clone, Copy, PartialEq, Eq)]
enum SplitAttribution {
    /// Segment time goes to the zone you left (original behaviour)
    #[default]
    Exit,
    /// Segment time goes to the zone you entered
    Entry,
}

#[derive(Gui)]
struct Settings {
    _general: Title,

    /// Path of Exile 2 Client.txt
    #[filter(("Log Files", "*.txt"))]
    client_log: FileSelect,

    /// Splits file (for Custom split mode)
    #[filter(("LiveSplit Splits", "*.lss"))]
    splits_file: FileSelect,

    /// Split mode
    split_mode: SplitMode,

    /// Which zone a segment's time is attributed to: the one you left (Exit)
    /// or the one you entered (Entry).
    split_attribution: SplitAttribution,

    /// Enable auto splitting
    #[default = true]
    auto_split: bool,

    /// Auto start new character runs when entering The Riverbank
    #[default = false]
    auto_start: bool,

    /// Verbose debug logging (per-tick line batches, level-ups)
    #[default = false]
    debug_log: bool,
}

// ---------------------------------------------------------------------------
// Client.txt tailing
// ---------------------------------------------------------------------------

/// Incrementally reads lines appended to a log file, mirroring the original
/// `ClientReader` (open, seek to end, poll for new lines). Detects the file
/// being truncated/recreated (new game session) and reopens from the start in
/// that case.
struct LogTail {
    path: String,
    reader: Option<BufReader<File>>,
    open_failure_reported: bool,
    /// Exponential backoff state for repeated failed opens (game not running).
    open_failures: u32,
    retry_at: Option<Instant>,
}

impl LogTail {
    fn new() -> Self {
        Self {
            path: String::new(),
            reader: None,
            open_failure_reported: false,
            open_failures: 0,
            retry_at: None,
        }
    }

    /// Points the tailer at `path`. Returns `true` when this resulted in a
    /// different file being watched than before. Empty paths are ignored so a
    /// transient settings read can never reset run state.
    fn sync_path(&mut self, path: &str) -> bool {
        if path.is_empty() || path == self.path {
            return false;
        }
        self.path.clear();
        self.path.push_str(path);
        self.reader = None;
        self.open_failures = 0;
        self.retry_at = None;
        true
    }

    /// Opens the file (once) and seeks to its end, like the original reader.
    fn ensure_open(&mut self) {
        if self.reader.is_some() || self.path.is_empty() {
            return;
        }
        if let Some(retry_at) = self.retry_at {
            if Instant::now() < retry_at {
                return;
            }
        }
        match File::open(&self.path) {
            Ok(mut file) => {
                let _ = file.seek(SeekFrom::End(0));
                asr::print_message("Watching Client.txt");
                self.open_failure_reported = false;
                self.open_failures = 0;
                self.retry_at = None;
                self.reader = Some(BufReader::new(file));
            }
            Err(err) => {
                // Report failures once per streak, then retry on an
                // exponential backoff (1s, 2s, 4s ... capped at 64s).
                if !self.open_failure_reported {
                    self.open_failure_reported = true;
                    asr::print_message(&format!("Client.txt open failed: {err}"));
                }
                let shift = self.open_failures.min(6);
                self.retry_at = Some(Instant::now() + Duration::from_secs(1 << shift));
                self.open_failures += 1;
            }
        }
    }

    /// Returns all complete lines appended since the last call. If the game
    /// recreated the log (truncation), the fresh file is reopened from the
    /// start and replayed - safe because already-seen areas/levels are
    /// suppressed by the run state, so in-progress runs survive restarts.
    fn poll_lines(&mut self) -> Vec<String> {
        let mut lines = Vec::new();

        loop {
            let Some(reader) = self.reader.as_mut() else {
                break;
            };

            let mut buf = Vec::new();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break, // EOF for now
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf);
                    let line = line.trim_end_matches(['\r', '\n']);
                    if !line.is_empty() {
                        lines.push(line.to_string());
                    }
                }
                Err(_) => {
                    self.reader = None;
                    break;
                }
            }
        }

        // Truncation / recreation check: if the file is now shorter than what
        // we already consumed, it was rewritten (game restarted).
        if let Some(reader) = self.reader.as_mut() {
            if let (Ok(pos), Ok(meta)) = (reader.stream_position(), reader.get_ref().metadata()) {
                if meta.len() < pos {
                    asr::print_message("Client.txt was recreated - replaying it (run state kept)");
                    match File::open(&self.path) {
                        // Deliberately NOT seeking to end: the replay is
                        // idempotent, and lines written between the recreation
                        // and this poll must not be skipped.
                        Ok(file) => *reader = BufReader::new(file),
                        Err(_) => self.reader = None,
                    }
                }
            }
        }

        lines
    }
}

// ---------------------------------------------------------------------------
// Log line parsing (ported from Component/ClientLog/ClientParser.cs)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event<'a> {
    /// `... Generating level N area "<id>"` - the player entered an area.
    /// The captured token is the **area ID** (e.g. `G1_town`), not the name.
    Area(&'a str),
    /// `... Got Instance Details` - instance handshake (load start).
    InstanceDetails,
    /// `... <player> is now level N`
    Level(u32),
    Nothing,
}

fn parse_line(line: &str) -> Event<'_> {
    // Timestamp prefix (`^[^ ]+ [^ ]+ (\d+)`) doesn't need to be parsed here:
    // events are consumed strictly in order, exactly like the C# parser.

    if let Some(id) = extract_area_id(line) {
        return Event::Area(id);
    }

    if line.contains("Got Instance Details") {
        return Event::InstanceDetails;
    }

    if let Some(level) = extract_level(line) {
        return Event::Level(level);
    }

    Event::Nothing
}

/// Matches `.*Generating level \d+ area "(.+)"` without pulling in a regex
/// engine. The capture is the area ID between the quotes (e.g. `G1_town`),
/// exactly what the original C# component feeds into its AreaId lookup.
fn extract_area_id(line: &str) -> Option<&str> {
    let idx = line.find("Generating level ")?;
    let rest = &line[idx..];

    // `Generating level <N> area "<id>" with seed <...>` - the ID spans from
    // the first to the last quote of the remainder.
    let start = rest.find('"')? + 1;
    let end = rest.rfind('"')?;
    if end <= start {
        return None;
    }
    Some(&rest[start..end])
}

/// Matches `.* is now level (\d+)$`.
fn extract_level(line: &str) -> Option<u32> {
    const MARKER: &str = " is now level ";
    let idx = line.rfind(MARKER)?;
    let tail = line[idx + MARKER.len()..].trim_end();
    if tail.is_empty() || !tail.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    tail.parse().ok()
}

// ---------------------------------------------------------------------------
// Run state machine (ported from Component/Timer/AutoSplitter.cs)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RunState {
    encountered_areas: HashSet<String>,
    levels_reached: HashSet<u32>,
}

impl RunState {
    fn clear(&mut self) {
        self.encountered_areas.clear();
        self.levels_reached.clear();
    }
}

// ---------------------------------------------------------------------------
// Custom mode: derive the trigger set from the user's splits file
// ---------------------------------------------------------------------------

/// The set of area IDs to split on in Custom mode, parsed from the segment
/// names of the user's `.lss` file. Re-parsed whenever the file changes.
struct SplitsTriggers {
    path: String,
    /// (file length, mtime nanos) of the last parse - cheap change detection.
    signature: Option<(u64, u64)>,
    ids: HashSet<String>,
    /// Area IDs in file order. The last one is the finish line: entering it
    /// ends the run.
    order: Vec<String>,
}

impl SplitsTriggers {
    fn new() -> Self {
        Self {
            path: String::new(),
            signature: None,
            ids: HashSet::new(),
            order: Vec::new(),
        }
    }

    /// Refreshes the trigger set if needed. Returns `true` when a new file
    /// was selected (so run state can be cleared). Empty paths are ignored so
    /// a transient settings read can never reset run state.
    fn update(&mut self, path: &str) -> bool {
        if path.is_empty() {
            return false;
        }
        if path != self.path {
            self.path.clear();
            self.path.push_str(path);
            self.signature = None;
            self.ids.clear();
            return true;
        }

        // Cheap change detection via metadata instead of re-reading each tick.
        let sig = File::open(&self.path).ok().and_then(|f| {
            let meta = f.metadata().ok()?;
            let mtime = meta
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?;
            Some((meta.len(), mtime.as_nanos() as u64))
        });

        match sig {
            Some(s) if Some(s) == self.signature => {} // unchanged
            Some(s) => {
                self.signature = Some(s);
                self.reparse();
            }
            None => {}
        }
        false
    }

    fn reparse(&mut self) {
        self.ids.clear();
        self.order.clear();
        let Ok(contents) = std::fs::read_to_string(&self.path) else {
            asr::print_message("Failed reading splits file for Custom mode");
            return;
        };

        let names = extract_segment_names(&contents);
        let mut unknown: Vec<&str> = Vec::new();
        for name in &names {
            match AREAS.iter().find(|a| a.name == *name) {
                Some(area) => {
                    self.ids.insert(area.id.to_string());
                    self.order.push(area.id.to_string());
                }
                None => unknown.push(name),
            }
        }

        if self.ids.is_empty() {
            asr::print_message(
                "Custom mode: no known PoE2 areas found in splits file - no area splits will fire",
            );
        } else {
            asr::print_message(&format!(
                "Custom mode: tracking {} areas from splits file",
                self.ids.len()
            ));
            for u in &unknown {
                asr::print_message(&format!("[dbg] segment without trigger: {u}"));
            }
        }
    }
}

/// Extracts the text of all `<Name>...</Name>` elements inside the
/// `<Segments>` section of an `.lss` document. Deliberately dependency-free:
/// the format is machine-written and regular enough for a plain scan.
fn extract_segment_names(contents: &str) -> Vec<String> {
    let region = match (
        contents.find("<Segments>"),
        contents.find("</Segments>"),
    ) {
        (Some(start), Some(end)) if start < end => &contents[start..end],
        _ => contents,
    };

    let mut names = Vec::new();
    let mut rest = region;
    while let Some(start) = rest.find("<Name>") {
        rest = &rest[start + "<Name>".len()..];
        let Some(end) = rest.find("</Name>") else { break };
        let raw = &rest[..end];
        rest = &rest[end..];

        let name = xml_unescape(raw.trim());
        if !name.is_empty() {
            names.push(name);
        }
    }
    names
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        // Must be last so "&amp;lt;" correctly becomes "&lt;".
        .replace("&amp;", "&")
}

/// Runs the per-tick run-state transitions, extracted from `main()` so tests
/// can drive ticks directly. Mirrors the original component's OnStart handler:
/// clear progress when a run truly starts or is reset. Pause/resume must NOT
/// count - it used to wipe encountered-areas on every resume. Unknown reports
/// are ignored entirely (no transition evaluated, prev unchanged).
///
/// In Entry mode the fresh-start point below owns the segment-1 split (D2).
fn on_timer_tick(
    settings: &Settings,
    triggers: &SplitsTriggers,
    prev_timer_state: &mut TimerState,
    state: &mut RunState,
    last_area: &mut Option<&'static Area>,
    start_split_done: &mut bool,
) {
    let current_timer_state = timer::state();
    if current_timer_state != TimerState::Unknown {
        let prev = *prev_timer_state;
        let fresh_start = current_timer_state == TimerState::Running
            && (prev == TimerState::NotRunning || prev == TimerState::Ended);
        let fresh_reset = current_timer_state == TimerState::NotRunning
            && prev != TimerState::NotRunning;
        if fresh_start || fresh_reset {
            asr::print_message(&format!(
                "[dbg] run state cleared ({current_timer_state:?} after {prev:?})"
            ));
            state.clear();
        }
        // Attempt-scoped locals reset on the run boundary only (log-path and
        // Custom-trigger changes clear them in main; every Ended tick below
        // covers ending without a reset). A fresh start must NOT clear them:
        // the hook needs to know where the player stands, and every path into
        // a fresh start (fresh_reset, Ended) has already wiped the previous
        // run's values.
        if fresh_reset {
            *last_area = None;
            *start_split_done = false;
        }
        if current_timer_state == TimerState::Ended {
            // Level-triggered: the player can leave to character select (no
            // area event) and press start before any reset - the stale finish
            // area must not be marked encountered in the next run.
            *last_area = None;
            *start_split_done = false;
        }

        // Fresh-start hook: the ONLY source of the segment-1 split in Entry
        // mode - the zone entry naming segment 1 happens before the timer
        // starts, so no area event can complete it (D2).
        if fresh_start
            && settings.split_attribution == SplitAttribution::Entry
            && settings.auto_split
            && settings.split_mode != SplitMode::Level
            && !*start_split_done
        {
            *start_split_done = true;

            // Split only where this mode splits areas at all: with no current
            // zone (char-select start) or an active-set one. Level,
            // AnyPercent-optional and Custom-absent never split from the
            // hook; auto_split was checked above (the hook lives in the main
            // loop, outside handle_line's guards). (`Option<&Area>` is Copy,
            // so filtering doesn't move the local.)
            let active = (*last_area).filter(|a| in_active_set(a, settings.split_mode, triggers));
            if (*last_area).is_none() || active.is_some() {
                timer::split(); // fires even when last_area is None (char-select start)
                asr::print_message("[dbg] split: run start (Entry)");
                if let Some(area) = active {
                    // Re-guard the standing zone: fresh_start's state.clear()
                    // just wiped the guard its entry had inserted.
                    state.encountered_areas.insert(area.id.to_string());
                }
            }
        }

        *prev_timer_state = current_timer_state;
    }
}

async fn main() {
    let mut settings = Settings::register();
    asr::print_message("Path of Exile 2 AutoSplitter (WASM port) loaded");

    let mut tail = LogTail::new();
    let mut state = RunState::default();
    let mut prev_timer_state = timer::state();
    let mut triggers = SplitsTriggers::new();
    let mut mismatch_warned = false;
    // Attempt-scoped runtime state (never persisted; re-learned from the log
    // each session): where the player last was (table-tracked area only) and
    // whether this run's segment-1 split already happened. Cleared together
    // on every reconfiguration and run boundary - see on_timer_tick.
    let mut last_area: Option<&'static Area> = None;
    let mut start_split_done = false;

    loop {
        settings.update();

        // React to the log path changing at runtime.
        if tail.sync_path(settings.client_log.path.as_str()) {
            state.clear();
            last_area = None;
            start_split_done = false;
        }
        // In Custom mode, keep the trigger set in sync with the splits file.
        // Other modes never read it, so skip the per-tick stat entirely.
        if settings.split_mode == SplitMode::Custom
            && triggers.update(settings.splits_file.path.as_str())
        {
            state.clear();
            last_area = None;
            start_split_done = false;
        }

        // Gentle one-shot hint for the silent failure mode: a configured
        // splits file does nothing unless Custom mode is selected.
        let file_configured = !settings.splits_file.path.is_empty();
        let mismatch = file_configured && settings.split_mode != SplitMode::Custom;
        if mismatch && !mismatch_warned {
            asr::print_message(
                "Splits file is set but split mode isn't Custom - the file won't be used",
            );
        }
        mismatch_warned = mismatch;
        tail.ensure_open();
        let lines = tail.poll_lines();

        on_timer_tick(
            &settings,
            &triggers,
            &mut prev_timer_state,
            &mut state,
            &mut last_area,
            &mut start_split_done,
        );

        for line in &lines {
            // handle_line hands back the area it saw; the loop tracks where
            // the player is even when no split fired (start area, guarded
            // re-entry). Unknown IDs and non-area events keep the old value.
            if let Some(area) = handle_line(line, &settings, &triggers, &mut state) {
                last_area = Some(area);
            }
        }
        if !lines.is_empty() && settings.debug_log {
            asr::print_message(&format!("[dbg] +{} new log lines", lines.len()));
        }

        next_tick().await;
    }
}

/// Whether this mode's active split set contains the area. Shared by
/// `handle_line`'s normal path and the fresh-start hook in `on_timer_tick`
/// so the two can never drift apart.
fn in_active_set(area: &Area, mode: SplitMode, triggers: &SplitsTriggers) -> bool {
    match mode {
        SplitMode::Full => true,
        SplitMode::AnyPercent => area.mandatory,
        SplitMode::Level => false,
        SplitMode::Custom => triggers.ids.contains(area.id),
    }
}

/// Processes one log line. Returns the known area the line placed the player
/// in (every known-area path, including the start branch) so the caller can
/// track `last_area`; `None` for auto-split-off, unknown IDs and non-area
/// events.
fn handle_line(
    line: &str,
    settings: &Settings,
    triggers: &SplitsTriggers,
    state: &mut RunState,
) -> Option<&'static Area> {
    if !settings.auto_split {
        return None;
    }

    match parse_line(line) {
        Event::Area(area_id) => {
            // Unknown IDs are ignored (with a debug trace so mismatches are
            // visible in log.txt instead of being silent).
            let Some(area) = AREAS.iter().find(|a| a.id == area_id) else {
                asr::print_message(&format!("[dbg] unknown area id: {area_id} ({line})"));
                return None;
            };

            // The start area never consumes a segment, regardless of how the
            // run was started (auto start or manual hotkey): the run begins
            // there, and checkpoint resets reload it without advancing.
            // With auto start enabled it also starts the timer.
            if area_id == START_AREA_ID {
                if settings.auto_start && timer::state() == TimerState::NotRunning {
                    timer::start();
                    asr::print_message("[dbg] start (The Riverbank)");
                }
                state.encountered_areas.insert(area_id.to_string());
                return Some(area);
            }

            // Only split if the active split set contains this area and we
            // haven't encountered it yet during this run.
            let in_active_set = in_active_set(area, settings.split_mode, triggers);

            if in_active_set && !state.encountered_areas.contains(area_id) {
                timer::split();
                asr::print_message(&format!("[dbg] split: {}", area.name));
            }

            // The finish line ends the run: Ziggurat Refuge for the campaign
            // modes, or the last segment of the splits file in Custom mode.
            // The extra split completes the finish-line segment itself.
            let finish_area = match settings.split_mode {
                SplitMode::Full | SplitMode::AnyPercent => Some(END_AREA_ID),
                SplitMode::Level => None,
                SplitMode::Custom => triggers.order.last().map(|s| s.as_str()),
            };
            if let Some(finish) = finish_area {
                if area_id == finish {
                    // Exit only: this extra split completes the finish-line
                    // segment itself. Under Entry the shifted alignment means
                    // the normal split above already ends the run at this load.
                    if settings.split_attribution == SplitAttribution::Exit {
                        timer::split();
                        asr::print_message("[dbg] split (finish line)");
                    }
                }
            }

            state.encountered_areas.insert(area_id.to_string());
            Some(area)
        }
        Event::Level(level) => {
            if settings.debug_log {
                asr::print_message(&format!("[dbg] level {level}"));
            }
            if settings.split_mode == SplitMode::Level && !state.levels_reached.contains(&level)
            {
                timer::split();
            }
            state.levels_reached.insert(level);
            None
        }
        Event::InstanceDetails | Event::Nothing => None,
    }
}

// ---------------------------------------------------------------------------
// Test harness: recording stubs for asr's host-inaccessible externs
// ---------------------------------------------------------------------------

/// Stubs for the `extern "C"` symbols asr imports from the LiveSplit host
/// (`asr/src/runtime/sys.rs`). On wasm they resolve to the runtime; a host
/// `cargo test` link would fail without them, so they live here, test-only.
///
/// Two rules (they make assertions count what LiveSplit actually does):
/// - `timer_split` records only while the timer reads `Running`: LiveSplit's
///   `Timer::split()` errors with `NoRunInProgress` without an active attempt
///   (livesplit-core `timing/timer/mod.rs:309`) and the host drops the error
///   (`auto_splitting/mod.rs:826`). Idle/ended splits must not be counted.
/// - everything is thread-local: libtest runs each test on its own thread, so
///   parallel tests cannot interleave recordings (a shared `static` would).
#[cfg(test)]
mod test_harness {
    use asr::timer::TimerState;
    use std::cell::{Cell, RefCell};

    /// A host call the splitter made into the timer API.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Call {
        Start,
        Split,
    }

    thread_local! {
        static TIMER_STATE: Cell<u32> = Cell::new(0); // 0 = NotRunning
        static CALLS: RefCell<Vec<Call>> = RefCell::new(Vec::new());
    }

    /// `sys::TimerState` is `#[repr(transparent)] u32` with NOT_RUNNING = 0,
    /// RUNNING = 1, PAUSED = 2, ENDED = 3; any other raw value reads back as
    /// `Unknown` in `asr::timer::state()`.
    fn raw(state: TimerState) -> u32 {
        match state {
            TimerState::NotRunning => 0,
            TimerState::Running => 1,
            TimerState::Paused => 2,
            TimerState::Ended => 3,
            _ => 4,
        }
    }

    /// Scripts what `timer::state()` will report to the code under test.
    pub fn set_timer_state(state: TimerState) {
        TIMER_STATE.with(|s| s.set(raw(state)));
    }

    /// Returns every timer call recorded so far, clearing the log.
    pub fn take_calls() -> Vec<Call> {
        CALLS.with(|c| c.borrow_mut().drain(..).collect())
    }

    fn record(call: Call) {
        CALLS.with(|c| c.borrow_mut().push(call));
    }

    #[no_mangle]
    pub extern "C" fn timer_get_state() -> u32 {
        TIMER_STATE.with(Cell::get)
    }

    #[no_mangle]
    pub extern "C" fn timer_start() {
        record(Call::Start);
        TIMER_STATE.with(|s| s.set(raw(TimerState::Running)));
    }

    #[no_mangle]
    pub extern "C" fn timer_split() {
        if TIMER_STATE.with(Cell::get) == 1 {
            record(Call::Split);
        }
    }

    #[no_mangle]
    pub extern "C" fn runtime_print_message(_text_ptr: *const u8, _text_len: usize) {}

    // ---- settings GUI registration (used by the D4 round-trip test) ------

    thread_local! {
        static SETTING_EVENTS: RefCell<Vec<String>> = RefCell::new(Vec::new());
    }

    fn record_setting(event: String) {
        SETTING_EVENTS.with(|e| e.borrow_mut().push(event));
    }

    fn str_of(ptr: *const u8, len: usize) -> String {
        // SAFETY: the bytes come from asr's wrappers, which pass &str slices.
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len) }).into_owned()
    }

    /// Settings GUI registration calls made so far ("kind:key[:value]").
    pub fn take_setting_events() -> Vec<String> {
        SETTING_EVENTS.with(|e| e.borrow_mut().drain(..).collect())
    }

    // Stubs mirror `asr/src/runtime/sys.rs` declarations; return values fake
    // an untouched host (defaults kept, no stored paths).

    #[no_mangle]
    pub extern "C" fn user_settings_add_bool(
        key_ptr: *const u8,
        key_len: usize,
        _desc_ptr: *const u8,
        _desc_len: usize,
        default_value: bool,
    ) -> bool {
        record_setting(format!("bool:{}", str_of(key_ptr, key_len)));
        default_value
    }

    #[no_mangle]
    pub extern "C" fn user_settings_add_title(
        key_ptr: *const u8,
        key_len: usize,
        _desc_ptr: *const u8,
        _desc_len: usize,
        _heading_level: u32,
    ) {
        record_setting(format!("title:{}", str_of(key_ptr, key_len)));
    }

    #[no_mangle]
    pub extern "C" fn user_settings_add_choice(
        key_ptr: *const u8,
        key_len: usize,
        _desc_ptr: *const u8,
        _desc_len: usize,
        default_ptr: *const u8,
        default_len: usize,
    ) {
        record_setting(format!(
            "choice:{}:{}",
            str_of(key_ptr, key_len),
            str_of(default_ptr, default_len)
        ));
    }

    #[no_mangle]
    pub extern "C" fn user_settings_add_choice_option(
        key_ptr: *const u8,
        key_len: usize,
        option_ptr: *const u8,
        option_len: usize,
        _opt_desc_ptr: *const u8,
        _opt_desc_len: usize,
    ) -> bool {
        record_setting(format!(
            "option:{}:{}",
            str_of(key_ptr, key_len),
            str_of(option_ptr, option_len)
        ));
        false // untouched host: no option chosen yet -> default wins
    }

    #[no_mangle]
    pub extern "C" fn user_settings_add_file_select(
        key_ptr: *const u8,
        key_len: usize,
        _desc_ptr: *const u8,
        _desc_len: usize,
    ) {
        record_setting(format!("file:{}", str_of(key_ptr, key_len)));
    }

    #[no_mangle]
    pub extern "C" fn user_settings_add_file_select_name_filter(
        key_ptr: *const u8,
        key_len: usize,
        _desc_ptr: *const u8,
        _desc_len: usize,
        _pattern_ptr: *const u8,
        _pattern_len: usize,
    ) {
        record_setting(format!("filefilter:{}", str_of(key_ptr, key_len)));
    }

    #[no_mangle]
    pub extern "C" fn user_settings_set_tooltip(
        key_ptr: *const u8,
        key_len: usize,
        _tooltip_ptr: *const u8,
        _tooltip_len: usize,
    ) {
        record_setting(format!("tooltip:{}", str_of(key_ptr, key_len)));
    }

    // Register() loads the settings map to read back FileSelect paths: fake
    // an empty map (non-zero handle, every key absent). Layout note: the map
    // handle is repr(transparent) over NonZeroU64 and get() returns
    // Option<NonZeroU64> - the stubs must match that niche, not Option<u64>.

    #[no_mangle]
    pub extern "C" fn settings_map_load() -> u64 {
        1
    }

    #[no_mangle]
    pub extern "C" fn settings_map_get(
        _map: u64,
        _key_ptr: *const u8,
        _key_len: usize,
    ) -> Option<std::num::NonZeroU64> {
        None
    }

    #[no_mangle]
    pub extern "C" fn settings_map_free(_map: u64) {}

    #[no_mangle]
    pub extern "C" fn setting_value_free(_value: u64) {}

    #[no_mangle]
    pub extern "C" fn setting_value_get_string(
        _value: u64,
        _buf_ptr: *mut u8,
        _buf_len_ptr: *mut usize,
    ) -> bool {
        false
    }

    #[no_mangle]
    pub extern "C" fn user_settings_add_file_select_mime_filter(
        key_ptr: *const u8,
        key_len: usize,
        _mime_ptr: *const u8,
        _mime_len: usize,
    ) {
        record_setting(format!("filemime:{}", str_of(key_ptr, key_len)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- T0 throwaway: handle_line links against the stubs ---------------

    #[test]
    fn throwaway_handle_line_links() {
        use test_harness::{set_timer_state, take_calls, Call};

        let settings = Settings {
            _general: Title,
            client_log: FileSelect {
                path: String::new(),
            },
            splits_file: FileSelect {
                path: String::new(),
            },
            split_mode: SplitMode::Full,
            split_attribution: SplitAttribution::Exit,
            auto_split: true,
            auto_start: false,
            debug_log: false,
        };
        let triggers = SplitsTriggers::new();
        let mut state = RunState::default();
        set_timer_state(TimerState::Running);

        handle_line(
            "2025/10/11 15:08:00 123 abcd [INFO Client 42] Generating level 3 area \"G1_13_2\" with seed 7",
            &settings,
            &triggers,
            &mut state,
        );
        assert_eq!(take_calls(), vec![Call::Split]);
    }

    // ---- xml_unescape ----------------------------------------------------

    #[test]
    fn unescape_handles_all_entities_and_ordering() {
        assert_eq!(xml_unescape("Traitor&apos;s Passage"), "Traitor's Passage");
        assert_eq!(xml_unescape("Jiquani&apos;s Sanctum"), "Jiquani's Sanctum");
        // &amp;lt; must become the literal text "&lt;", not "<".
        assert_eq!(xml_unescape("&amp;lt;"), "&lt;");
        assert_eq!(xml_unescape("a &amp; b &lt;c&gt; &quot;d&quot;"), "a & b <c> \"d\"");
    }

    // ---- extract_segment_names -------------------------------------------

    const PRETTY: &str = "\
<Run version=\"1.7.0\">
  <GameName>Path of Exile 2</GameName>
  <Segments>
    <Segment>
      <Name>The Riverbank</Name>
      <IconImage>AAAA</IconImage>
    </Segment>
    <Segment>
      <Name>Traitor&apos;s Passage</Name>
      <IconImage />
    </Segment>
    <Segment>
      <Name></Name>
    </Segment>
  </Segments>
</Run>";

    #[test]
    fn segment_names_pretty_printed() {
        let names = extract_segment_names(PRETTY);
        assert_eq!(names, vec!["The Riverbank", "Traitor's Passage"]);
    }

    #[test]
    fn segment_names_compact_resaved() {
        let compact = "<?xml?><Run version=\"1.8.0\"><GameName>P</GameName><Segments>\
             <Segment><Name>The Riverbank</Name><Icon/></Segment>\
             <Segment><Name>Act 1 Boss</Name><Icon/></Segment>\
           </Segments><AutoSplitterSettings/></Run>";
        let names = extract_segment_names(compact);
        assert_eq!(names, vec!["The Riverbank", "Act 1 Boss"]);
    }

    #[test]
    fn segment_names_without_segments_wrapper_scans_whole_doc() {
        assert_eq!(extract_segment_names("<Name>Solo</Name>"), vec!["Solo"]);
        assert!(extract_segment_names("<nothing/>").is_empty());
    }

    // ---- extract_area_id / extract_level / parse_line ---------------------

    const AREA_LINE: &str = "2026/08/25 15:49:58 8976354 2caa2332 [DEBUG Client 332] Generating level 12 area \"G1_13_2\" with seed 3742364270";
    const LEVEL_LINE: &str =
        "2026/08/25 16:38:16 11979831 3ef231e0 [INFO Client 332] : CHaospracs (Warrior) is now level 10";
    const INSTANCE_LINE: &str =
        "2026/08/25 03:48:39 31479523 2d8e6512 [DEBUG Client 332] Got Instance Details from login server";

    #[test]
    fn area_id_extracted_from_real_line() {
        assert_eq!(extract_area_id(AREA_LINE), Some("G1_13_2"));
    }

    #[test]
    fn area_id_none_without_marker() {
        assert_eq!(extract_area_id(INSTANCE_LINE), None);
    }

    #[test]
    fn level_extracted_and_anchored() {
        assert_eq!(extract_level(LEVEL_LINE), Some(10));
        assert_eq!(parse_line(LEVEL_LINE), Event::Level(10));
        // Trailing junk after the number breaks the `$` anchor semantics.
        let noisy = "2026/08/25 16:38:16 1 abc [INFO Client 332] : x is now level 10 soon";
        assert_eq!(extract_level(noisy), None);
    }

    #[test]
    fn parse_line_classifies() {
        assert_eq!(parse_line(AREA_LINE), Event::Area("G1_13_2"));
        assert_eq!(parse_line(INSTANCE_LINE), Event::InstanceDetails);
        assert_eq!(parse_line("chat: hi"), Event::Nothing);
    }

    // ---- Custom trigger parsing end-to-end on a hybrid fixture ------------

    #[test]
    fn custom_hybrid_matches_only_known_areas_in_order() {
        let doc = "<Segments>\
             <Segment><Name>The Grim Tangle</Name></Segment>\
             <Segment><Name>Act 1 Boss</Name></Segment>\
             <Segment><Name>Hunting Grounds</Name></Segment>\
             <Segment><Name>The Ziggurat Refuge</Name></Segment>\
           </Segments>";
        // Same primitives reparse() uses: names -> ids via the area table.
        let ids: Vec<&'static str> = extract_segment_names(doc)
            .iter()
            .filter_map(|n| AREAS.iter().find(|a| a.name == *n).map(|a| a.id))
            .collect();
        assert_eq!(ids, vec!["G1_6", "G1_11", "G_Endgame_Town"]);
    }

    // ---- T1 characterization goldens (recorded against pre-Entry code) ---
    //
    // These freeze what the CURRENT splitter does. The T2 Entry change must
    // leave every one of them byte-identical (Exit is the default mode).

    use test_harness::Call;

    fn settings_full() -> Settings {
        Settings {
            _general: Title,
            client_log: FileSelect {
                path: String::new(),
            },
            splits_file: FileSelect {
                path: String::new(),
            },
            split_mode: SplitMode::Full,
            split_attribution: SplitAttribution::Exit,
            auto_split: true,
            auto_start: false,
            debug_log: false,
        }
    }

    /// Log-line shapes copied from the real fixture (marker + quotes around
    /// the id is all `parse_line` reads).
    fn area_line(id: &str) -> String {
        format!(
            "2025/10/11 15:10:21 1372614 2caa1a34 [DEBUG Client 448] \
             Generating level 1 area \"{id}\" with seed 2708865591"
        )
    }
    fn level_line(n: u32) -> String {
        format!(
            "2025/10/11 15:13:36 1567527 3ef232c2 [INFO Client 448] \
             TestChar is now level {n}"
        )
    }
    // INSTANCE_LINE (defined above for the parser tests) doubles as feed.

    /// Drives the real decision path: lines through `handle_line`, ticks
    /// through `on_timer_tick`, recording what reaches the timer API. Mirrors
    /// main(): `line` stores the area handle_line hands back, so `last_area`
    /// stays faithful to what the production loop would track.
    struct Scenario {
        settings: Settings,
        triggers: SplitsTriggers,
        state: RunState,
        prev: TimerState,
        last_area: Option<&'static Area>,
        start_split_done: bool,
    }

    impl Scenario {
        /// Timer reads `Running` (mid-run attach): the line-driven default.
        fn new(settings: Settings) -> Self {
            test_harness::set_timer_state(TimerState::Running);
            Self {
                settings,
                triggers: SplitsTriggers::new(),
                state: RunState::default(),
                prev: TimerState::Running,
                last_area: None,
                start_split_done: false,
            }
        }
        fn line(&mut self, line: &str) -> Vec<Call> {
            if let Some(area) =
                handle_line(line, &self.settings, &self.triggers, &mut self.state)
            {
                self.last_area = Some(area);
            }
            test_harness::take_calls()
        }
        fn area(&mut self, id: &str) -> Vec<Call> {
            self.line(&area_line(id))
        }
        fn tick(&mut self) -> Vec<Call> {
            on_timer_tick(
                &self.settings,
                &self.triggers,
                &mut self.prev,
                &mut self.state,
                &mut self.last_area,
                &mut self.start_split_done,
            );
            test_harness::take_calls()
        }
        /// Scripts what `timer::state()` reports to the next tick/line.
        fn script(&mut self, state: TimerState) {
            test_harness::set_timer_state(state);
        }
        /// Attaches with the timer idle: the prev tick observed NotRunning.
        fn idle(&mut self) {
            self.script(TimerState::NotRunning);
            self.tick();
        }
        /// The NotRunning -> Running transition a run start produces, plus the
        /// tick that observes it - the fresh start the Entry hook reacts to.
        fn start_run(&mut self) -> Vec<Call> {
            self.prev = TimerState::NotRunning;
            test_harness::set_timer_state(TimerState::Running);
            self.tick()
        }
    }

    #[test]
    fn golden_start_area_never_splits() {
        let mut sc = Scenario::new(settings_full());
        assert!(sc.area("G1_1").is_empty());
        assert!(sc.area("G1_1").is_empty()); // checkpoint reload of start zone
    }

    #[test]
    fn golden_auto_start_starts_the_timer() {
        let mut s = settings_full();
        s.auto_start = true;
        let mut sc = Scenario::new(s);
        sc.script(TimerState::NotRunning);
        assert_eq!(sc.area("G1_1"), vec![Call::Start]); // stub flips to Running
        assert!(sc.area("G1_1").is_empty()); // running now: no second start
    }

    #[test]
    fn golden_finish_double_splits_then_extra_alone() {
        let mut sc = Scenario::new(settings_full());
        // First entry: normal split + Exit-only extra (current behaviour).
        assert_eq!(sc.area("G_Endgame_Town"), vec![Call::Split, Call::Split]);
        // Re-entry: the extra has no `encountered` gate - it fires again.
        assert_eq!(sc.area("G_Endgame_Town"), vec![Call::Split]);
    }

    #[test]
    fn golden_town_reentry_guarded() {
        let mut sc = Scenario::new(settings_full());
        assert_eq!(sc.area("G1_town"), vec![Call::Split]);
        assert!(sc.area("G1_town").is_empty());
    }

    #[test]
    fn golden_unknown_id_is_inert() {
        let mut sc = Scenario::new(settings_full());
        assert!(sc.area("Zzz_not_a_real_area").is_empty());
        assert_eq!(sc.area("G1_town"), vec![Call::Split]); // state untouched
    }

    #[test]
    fn golden_level_mode_splits_once_per_new_level() {
        let mut s = settings_full();
        s.split_mode = SplitMode::Level;
        let mut sc = Scenario::new(s);
        assert_eq!(sc.line(&level_line(2)), vec![Call::Split]);
        assert!(sc.line(&level_line(2)).is_empty()); // duplicate: guarded
        assert_eq!(sc.line(&level_line(3)), vec![Call::Split]);
        assert!(sc.line(INSTANCE_LINE).is_empty());
    }

    #[test]
    fn golden_levels_are_inert_outside_level_mode() {
        let mut sc = Scenario::new(settings_full());
        assert!(sc.line(&level_line(2)).is_empty());
        assert!(sc.line(&level_line(3)).is_empty());
    }

    #[test]
    fn golden_full_campaign_walk() {
        let mut sc = Scenario::new(settings_full());
        let mut calls = Vec::new();
        for area in AREAS {
            calls.extend(sc.area(area.id));
        }
        // start area suppressed, every other tracked area splits once,
        // the finish entry adds the Exit-only extra: 95 + 1 = 96
        assert_eq!(calls, vec![Call::Split; 96]);
    }

    #[test]
    fn golden_anypercent_walk_splits_mandatory_only() {
        let mut s = settings_full();
        s.split_mode = SplitMode::AnyPercent;
        let mut sc = Scenario::new(s);
        let mut calls = Vec::new();
        for area in AREAS {
            calls.extend(sc.area(area.id));
        }
        // start suppressed; only mandatory areas split; finish entry adds the
        // Exit-only extra (recorded from current code)
        assert_eq!(calls, vec![Call::Split; 78]);
    }

    #[test]
    fn golden_run_boundaries_and_pause() {
        let mut sc = Scenario::new(settings_full());
        assert_eq!(sc.area("G1_town"), vec![Call::Split]);

        // Pause/resume: no state wipe, guard intact, no calls.
        sc.script(TimerState::Paused);
        assert!(sc.tick().is_empty());
        sc.script(TimerState::Running);
        assert!(sc.tick().is_empty());
        assert!(sc.area("G1_town").is_empty());

        // Reset (Running -> NotRunning) wipes; NotRunning -> Running is a
        // fresh start and wipes again (no hook exists in Exit mode).
        sc.script(TimerState::NotRunning);
        assert!(sc.tick().is_empty());
        sc.script(TimerState::Running);
        assert!(sc.tick().is_empty());
        assert_eq!(sc.area("G1_town"), vec![Call::Split]); // wiped -> splits
    }

    #[test]
    fn golden_fixture_replay() {
        let mut sc = Scenario::new(settings_full());
        let mut out = String::new();
        for line in include_str!("../testdata/real-run.txt").lines() {
            if line.starts_with('#') {
                continue;
            }
            for call in sc.line(line) {
                out.push_str(match call {
                    Call::Start => "start\n",
                    Call::Split => "split\n",
                });
            }
        }
        assert_eq!(out, include_str!("../testdata/real-run.golden"));
    }

    // ---- T2: Entry mode ---------------------------------------------------

    fn settings_entry() -> Settings {
        let mut s = settings_full();
        s.split_attribution = SplitAttribution::Entry;
        s
    }

    /// Appends `label` plus any timer calls it triggered (lowercased), so an
    /// assertion reads as an interleaved table of entries and splits.
    fn record(trace: &mut Vec<String>, label: &str, calls: Vec<Call>) {
        trace.push(label.to_string());
        trace.extend(calls.iter().map(|c| format!("{c:?}").to_lowercase()));
    }

    /// Builds a trigger set from segment names exactly like `reparse()` does.
    fn triggers_from_names(names: &[&str]) -> SplitsTriggers {
        let mut triggers = SplitsTriggers::new();
        for name in names {
            if let Some(area) = AREAS.iter().find(|a| a.name == *name) {
                triggers.ids.insert(area.id.to_string());
                triggers.order.push(area.id.to_string());
            }
        }
        triggers
    }

    /// The plan's "Act1-only shape": any% segments 1..13 (The Riverbank ->
    /// The Manor Ramparts, Campaign-AnyPercent.lss verbatim) plus Vastiri
    /// Outskirts as the finish - 14 segments ending at the act boundary.
    fn act1_only_triggers() -> SplitsTriggers {
        let doc = include_str!("../../splits/Campaign-AnyPercent.lss");
        let names = extract_segment_names(doc);
        let mut picked: Vec<&str> = names.iter().take(13).map(|s| s.as_str()).collect();
        picked.push("Vastiri Outskirts");
        triggers_from_names(&picked)
    }

    #[test]
    fn entry_table_attribution_shifts_by_one_zone() {
        // Entry: the hook spends segment 1 at ~0, then each entry completes
        // the segment named by that entry (time since the previous entry).
        let mut sc = Scenario::new(settings_entry());
        sc.idle();
        let mut trace = Vec::new();
        record(&mut trace, "run start", sc.start_run());
        record(&mut trace, "enter G1_1", sc.area("G1_1"));
        record(&mut trace, "enter G1_town", sc.area("G1_town"));
        record(&mut trace, "enter G1_2", sc.area("G1_2"));
        assert_eq!(
            trace,
            [
                "run start", "split",     // segment 1 (Riverbank) = 0:00
                "enter G1_1",             // start area: never splits (D3)
                "enter G1_town", "split", // segment 2 receives Riverbank time
                "enter G1_2", "split",    // segment 3 receives town time
            ]
        );

        // Exit, same path: splits lag one zone behind.
        let mut sc = Scenario::new(settings_full());
        sc.idle();
        let mut trace = Vec::new();
        record(&mut trace, "run start", sc.start_run());
        record(&mut trace, "enter G1_1", sc.area("G1_1"));
        record(&mut trace, "enter G1_town", sc.area("G1_town"));
        record(&mut trace, "enter G1_2", sc.area("G1_2"));
        assert_eq!(
            trace,
            [
                "run start", // no hook in Exit
                "enter G1_1",
                "enter G1_town", "split", // segment 1 completes on leaving it
                "enter G1_2", "split",
            ]
        );
    }

    #[test]
    fn entry_finish_normal_split_only_no_extra() {
        let mut sc = Scenario::new(settings_entry()); // attached, Running
        assert_eq!(sc.area("G_Endgame_Town"), vec![Call::Split]); // ONE split
        assert!(sc.area("G_Endgame_Town").is_empty()); // no Exit-style extra
    }

    #[test]
    fn entry_full_campaign_walk_same_count_as_exit() {
        let mut sc = Scenario::new(settings_entry());
        sc.idle();
        let mut calls = sc.start_run(); // hook: segment 1
        for area in AREAS {
            calls.extend(sc.area(area.id));
        }
        // hook + 95 non-start entries = 96 = Exit's count (its extra finish
        // split replaced by the hook): N segments -> N auto splits.
        assert_eq!(calls, vec![Call::Split; 96]);
    }

    #[test]
    fn entry_act1_only_14_splits_ends_at_vastiri() {
        let mut s = settings_entry();
        s.split_mode = SplitMode::Custom;
        let mut sc = Scenario::new(s);
        sc.triggers = act1_only_triggers();
        assert_eq!(sc.triggers.order.len(), 14);

        sc.idle();
        let mut trace = Vec::new();
        record(&mut trace, "run start", sc.start_run());
        let order = sc.triggers.order.clone();
        for id in &order {
            record(&mut trace, &format!("enter {id}"), sc.area(id));
        }
        let splits = trace.iter().filter(|e| e.as_str() == "split").count();
        assert_eq!(splits, 14); // hook + 12 mid zones + the finish entry
        let tail: Vec<&str> = trace[trace.len() - 2..].iter().map(String::as_str).collect();
        assert_eq!(tail, ["enter G2_1", "split"]); // run ends at Vastiri
        assert!(sc.area("G2_1").is_empty()); // Entry: no finish extra anywhere
    }

    #[test]
    fn entry_run_boundary_finish_splits_in_next_run() {
        let mut sc = Scenario::new(settings_entry());
        sc.idle();
        assert_eq!(sc.start_run(), vec![Call::Split]); // run 1 hook
        assert_eq!(sc.area("G_Endgame_Town"), vec![Call::Split]); // run 1 finish

        sc.script(TimerState::Ended);
        assert!(sc.tick().is_empty()); // Ended: locals cleared, no split
        sc.script(TimerState::NotRunning);
        assert!(sc.tick().is_empty()); // fresh_reset: run state cleared

        // New run starts with no zone in between...
        assert_eq!(sc.start_run(), vec![Call::Split]);
        // ...so the previous run's finish is not marked encountered and
        // entering it later still splits.
        assert_eq!(sc.area("G_Endgame_Town"), vec![Call::Split]);
    }

    #[test]
    fn entry_two_consecutive_runs_identical() {
        fn walk(sc: &mut Scenario) -> Vec<Call> {
            let mut calls = sc.start_run();
            for id in ["G1_1", "G1_town", "G1_2", "G_Endgame_Town"] {
                calls.extend(sc.area(id));
            }
            calls
        }
        let mut sc = Scenario::new(settings_entry());
        sc.idle();
        let run1 = walk(&mut sc);
        sc.script(TimerState::Ended);
        sc.tick();
        sc.script(TimerState::NotRunning);
        sc.tick();
        let run2 = walk(&mut sc);
        assert_eq!(run1, run2);
        assert_eq!(run1, vec![Call::Split; 4]); // hook + town + Clearfell + finish
    }

    #[test]
    fn entry_pause_reload_resume_single_segment1_split() {
        let mut sc = Scenario::new(settings_entry());
        sc.idle();
        let mut all = sc.start_run(); // THE segment-1 split
        all.extend(sc.area("G1_town")); // progressed: split 2

        sc.script(TimerState::Paused);
        assert!(sc.tick().is_empty()); // pause observed: nothing
        assert!(sc.area("G1_town").is_empty()); // die + reload: guarded
        assert!(sc.area("G1_1").is_empty()); // respawn at start: no split

        sc.script(TimerState::Running);
        assert!(sc.tick().is_empty()); // resume (prev=Paused): no hook
        assert!(sc.area("G1_town").is_empty()); // reload again: still guarded

        assert_eq!(all, vec![Call::Split; 2]); // exactly one segment-1 split
    }

    #[test]
    fn entry_manual_start_stands_in_zone_guard_reload() {
        let mut sc = Scenario::new(settings_entry());
        sc.idle();
        assert!(sc.area("G1_town").is_empty()); // idle entry: timer not running
        // fresh start: state.clear() wipes the guard, but last_area survives
        // the boundary -> the hook re-guards as it splits.
        assert_eq!(sc.start_run(), vec![Call::Split]);
        assert!(sc.area("G1_town").is_empty()); // reload: no phantom
    }

    #[test]
    fn entry_reset_start_reload_phantom_documented() {
        // Qualified: the hook only re-guards when last_area was populated
        // before the start (previous test). A mid-run reset clears it at the
        // boundary, so reset -> start -> reload-same-zone phantom-splits -
        // documented parity with today's Exit mode, accepted, not a regression.
        let mut sc = Scenario::new(settings_entry());
        sc.idle();
        sc.start_run();
        assert_eq!(sc.area("G1_town"), vec![Call::Split]); // standing there
        sc.script(TimerState::NotRunning);
        assert!(sc.tick().is_empty()); // reset clears everything
        assert_eq!(sc.start_run(), vec![Call::Split]); // last_area is None now
        assert_eq!(sc.area("G1_town"), vec![Call::Split]); // PHANTOM

        // The same phantom exists in Exit mode today:
        let mut ex = Scenario::new(settings_full());
        ex.idle();
        ex.area("G1_town");
        ex.script(TimerState::NotRunning);
        ex.tick();
        ex.script(TimerState::Running);
        ex.tick(); // fresh start (Exit: no hook, state cleared)
        assert_eq!(ex.area("G1_town"), vec![Call::Split]); // identical phantom
    }

    #[test]
    fn entry_char_select_start_loading_lands_on_segment2() {
        let mut sc = Scenario::new(settings_entry());
        sc.idle(); // char select: last_area is None
        let mut calls = sc.start_run(); // hook splits, nothing to re-guard
        assert!(sc.last_area.is_none());
        calls.extend(sc.area("G1_1")); // load into the start zone: no split
        assert_eq!(calls, vec![Call::Split]); // still just segment 1
        calls.extend(sc.area("G1_town")); // segment 2 completes (incl. loading)
        assert_eq!(calls, vec![Call::Split; 2]);
    }

    #[test]
    fn entry_hook_level_mode_never_splits() {
        let mut s = settings_entry();
        s.split_mode = SplitMode::Level;
        let mut sc = Scenario::new(s);
        sc.idle();
        assert!(sc.start_run().is_empty()); // no hook in Level mode (D6)
        assert_eq!(sc.line(&level_line(2)), vec![Call::Split]); // levels work
    }

    #[test]
    fn entry_hook_anypercent_optional_zone_never_splits() {
        let mut s = settings_entry();
        s.split_mode = SplitMode::AnyPercent;
        let mut sc = Scenario::new(s);
        sc.idle();
        assert!(sc.area("G1_3").is_empty()); // stand in Mud Burrow (optional)
        assert!(sc.start_run().is_empty()); // hook filtered out
        assert_eq!(sc.area("G1_4"), vec![Call::Split]); // progression splits
    }

    #[test]
    fn entry_hook_custom_absent_zone_never_splits() {
        let mut s = settings_entry();
        s.split_mode = SplitMode::Custom;
        let mut sc = Scenario::new(s);
        sc.triggers = triggers_from_names(&["Clearfell Encampment"]);
        sc.idle();
        assert!(sc.area("G1_2").is_empty()); // stand in a zone the file lacks
        assert!(sc.start_run().is_empty()); // hook filtered out
        assert_eq!(sc.area("G1_town"), vec![Call::Split]); // in-file splits
    }

    #[test]
    fn entry_auto_split_off_at_start_no_late_hook() {
        let mut sc = Scenario::new(settings_entry());
        sc.settings.auto_split = false;
        sc.idle();
        assert!(sc.start_run().is_empty()); // gated at the fresh start
        sc.settings.auto_split = true; // enabled afterwards...
        sc.script(TimerState::Unknown);
        assert!(sc.tick().is_empty()); // ...an Unknown gap...
        sc.script(TimerState::Running);
        assert!(sc.tick().is_empty()); // ...cannot fire it late
        assert_eq!(sc.area("G1_town"), vec![Call::Split]); // Exit-aligned run
    }

    #[test]
    fn entry_auto_start_fires_exactly_one_segment1_split() {
        let mut s = settings_entry();
        s.auto_start = true;
        let mut sc = Scenario::new(s);
        sc.idle(); // idle at the start area
        assert_eq!(sc.area("G1_1"), vec![Call::Start]); // auto start flips Running
        assert_eq!(sc.tick(), vec![Call::Split]); // next tick: fresh start hook
        assert!(sc.area("G1_1").is_empty()); // reload: no split
        assert_eq!(sc.area("G1_town"), vec![Call::Split]); // segment 2
    }

    #[test]
    fn entry_attach_while_running_never_fires_the_hook() {
        let mut sc = Scenario::new(settings_entry()); // prev = Running: no fresh start
        assert!(sc.tick().is_empty());
        // First split is a plain normal split = Exit alignment for this run.
        assert_eq!(sc.area("G1_town"), vec![Call::Split]);
        assert!(sc.tick().is_empty());
    }

    #[test]
    fn split_attribution_registers_exit_as_default() {
        // D4: the derive registers the new key exactly like split_mode, so
        // existing .lss files (which lack it) keep loading as Exit.
        let registered = Settings::register();
        // (plain == : SplitAttribution derives exactly what SplitMode does, no Debug)
        assert!(registered.split_attribution == SplitAttribution::Exit);
        let events = test_harness::take_setting_events();
        assert!(events.iter().any(|e| e == "choice:split_attribution:Exit"));
        assert!(events.iter().any(|e| e == "option:split_attribution:Exit"));
        assert!(events.iter().any(|e| e == "option:split_attribution:Entry"));
        assert!(events.iter().any(|e| e == "choice:split_mode:Full")); // untouched
    }
}
