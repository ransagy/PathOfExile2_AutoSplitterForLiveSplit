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

async fn main() {
    let mut settings = Settings::register();
    asr::print_message("Path of Exile 2 AutoSplitter (WASM port) loaded");

    let mut tail = LogTail::new();
    let mut state = RunState::default();
    let mut prev_timer_state = timer::state();
    let mut triggers = SplitsTriggers::new();
    let mut mismatch_warned = false;

    loop {
        settings.update();

        // React to the log path changing at runtime.
        if tail.sync_path(settings.client_log.path.as_str()) {
            state.clear();
        }
        // In Custom mode, keep the trigger set in sync with the splits file.
        // Other modes never read it, so skip the per-tick stat entirely.
        if settings.split_mode == SplitMode::Custom
            && triggers.update(settings.splits_file.path.as_str())
        {
            state.clear();
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

        // Mirror the original component's OnStart handler: clear progress when
        // a run truly starts or is reset. Pause/resume must NOT count - it
        // used to wipe encountered-areas on every resume. Unknown reports are
        // ignored entirely (no transition evaluated, prev unchanged).
        let current_timer_state = timer::state();
        if current_timer_state != TimerState::Unknown {
            let fresh_start = current_timer_state == TimerState::Running
                && (prev_timer_state == TimerState::NotRunning
                    || prev_timer_state == TimerState::Ended);
            let fresh_reset = current_timer_state == TimerState::NotRunning
                && prev_timer_state != TimerState::NotRunning;
            if fresh_start || fresh_reset {
                asr::print_message(&format!(
                    "[dbg] run state cleared ({current_timer_state:?} after {prev_timer_state:?})"
                ));
                state.clear();
            }
            prev_timer_state = current_timer_state;
        }

        for line in &lines {
            handle_line(line, &settings, &triggers, &mut state);
        }
        if !lines.is_empty() && settings.debug_log {
            asr::print_message(&format!("[dbg] +{} new log lines", lines.len()));
        }

        next_tick().await;
    }
}

fn handle_line(
    line: &str,
    settings: &Settings,
    triggers: &SplitsTriggers,
    state: &mut RunState,
) {
    if !settings.auto_split {
        return;
    }

    match parse_line(line) {
        Event::Area(area_id) => {
            // Unknown IDs are ignored (with a debug trace so mismatches are
            // visible in log.txt instead of being silent).
            let Some(area) = AREAS.iter().find(|a| a.id == area_id) else {
                asr::print_message(&format!("[dbg] unknown area id: {area_id} ({line})"));
                return;
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
                return;
            }

            // Only split if the active split set contains this area and we
            // haven't encountered it yet during this run.
            let in_active_set = match settings.split_mode {
                SplitMode::Full => true,
                SplitMode::AnyPercent => area.mandatory,
                SplitMode::Level => false,
                SplitMode::Custom => triggers.ids.contains(area_id),
            };

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
                    timer::split();
                    asr::print_message("[dbg] split (finish line)");
                }
            }

            state.encountered_areas.insert(area_id.to_string());
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
        }
        Event::InstanceDetails | Event::Nothing => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
