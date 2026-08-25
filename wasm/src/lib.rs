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

const START_AREA: &str = "The Riverbank";
const END_AREA: &str = "The Ziggurat Refuge";

struct Area {
    name: &'static str,
    /// Whether the area is part of the mandatory (Any%) route.
    mandatory: bool,
}

macro_rules! areas {
    ($($name:literal, $mandatory:literal);* $(;)?) => {
        &[ $( Area { name: $name, mandatory: $mandatory } ),* ]
    };
}

/// All tracked campaign areas. Order does not matter; splitting is driven by
/// the order areas are entered in-game (same linear behaviour as the original
/// component).
static AREAS: &[Area] = areas! {
    // Act 1
    "The Riverbank", true;
    "Clearfell Encampment", true;
    "Clearfell", true;
    "Mud Burrow", false;
    "The Grelwood", true;
    "The Red Vale", true;
    "The Grim Tangle", true;
    "Cemetery of the Eternals", true;
    "Mausoleum of the Praetor", true;
    "Tomb of the Consort", true;
    "Hunting Grounds", true;
    "Freythorn", false;
    "Ogham Farmlands", true;
    "Ogham Village", true;
    "The Manor Ramparts", true;
    "Ogham Manor", true;
    // Act 2
    "Vastiri Outskirts", true;
    "The Ardura Caravan", true;
    "Mawdun Quarry", true;
    "Mawdun Mine", true;
    "Traitor's Passage", true;
    "The Halani Gates (blocked)", true;
    "The Halani Gates", true;
    "Keth", true;
    "The Lost City", true;
    "Buried Shrines", true;
    "Mastodon Badlands", true;
    "Lightless Passage", false;
    "The Well of Souls", false;
    "The Bone Pits", true;
    "Valley of the Titans", true;
    "The Titan Grotto", true;
    "Deshar", true;
    "Path of Mourning", true;
    "The Spires of Deshar", true;
    "Dreadnought", true;
    "Trial of the Sekhemas", false;
    // Act 3
    "Sandswept Marsh", true;
    "Ziggurat Encampment", true;
    "Jungle Ruins", true;
    "The Venom Crypts", false;
    "Infested Barrens", true;
    "The Azak Bog", false;
    "Chimeral Wetlands", true;
    "Jiquani's Machinarium", true;
    "Jiquani's Sanctum", true;
    "The Matlan Waterways", true;
    "The Drowned City", true;
    "The Molten Vault", false;
    "Apex of Filth", true;
    "Temple of Kopec", true;
    "Utzaal", true;
    "Aggorat", true;
    "The Black Chambers", true;
    "The Temple of Chaos", false;
    // Act 4
    "Kingsmarch", true;
    "Isle of Kin", true;
    "Volcanic Warrens", true;
    "Eye of Hinekora", false;
    "Halls of the Dead", false;
    "Trial of the Ancestors", false;
    "Kedge Bay", false;
    "Journey's End", false;
    "Abandoned Prison", true;
    "Solitary Confinement", true;
    "Whakapanu Island", false;
    "Singing Caverns", false;
    "Shrike Island", true;
    "Arastas", true;
    "The Excavation", true;
    "Ngakanu", true;
    "Heart of the Tribe", true;
    "Plunder's Point", false;
    // Interlude 1
    "The Refuge", true;
    "Scorched Farmlands", true;
    "Stones of Serle", true;
    "The Blackwood", true;
    "Holten", true;
    "Wolvenhold", true;
    "Holten Estate", true;
    // Interlude 2
    "The Khari Bazaar", true;
    "The Khari Crossing", true;
    "Pools of Khatal", true;
    "Sel Khari Sanctuary", true;
    "The Galai Gates", true;
    "Qimah", true;
    "Qimah Reservoir", true;
    // Interlude 3
    "The Glade", true;
    "Ashen Forest", true;
    "Kriar Village", true;
    "Glacial Tarn", true;
    "Howling Caves", false;
    "Kriar Peaks", true;
    "Etched Ravine", true;
    "The Cuachic Vault", true;
    // Endgame
    "The Ziggurat Refuge", true;
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
}

#[derive(Gui)]
struct Settings {
    _general: Title,

    /// Path of Exile 2 Client.txt
    #[filter(("Log Files", "*.txt"))]
    client_log: FileSelect,

    /// Split mode
    split_mode: SplitMode,

    /// Enable auto splitting
    #[default = true]
    auto_split: bool,

    /// Auto start new character runs when entering The Riverbank
    #[default = false]
    auto_start: bool,
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
}

impl LogTail {
    fn new() -> Self {
        Self {
            path: String::new(),
            reader: None,
        }
    }

    /// Points the tailer at `path`. Returns `true` when this resulted in a
    /// different file being watched than before.
    fn sync_path(&mut self, path: &str) -> bool {
        if path == self.path {
            return false;
        }
        self.path.clear();
        self.path.push_str(path);
        self.reader = None;
        !self.path.is_empty()
    }

    /// Opens the file (once) and seeks to its end, like the original reader.
    fn ensure_open(&mut self) {
        if self.reader.is_some() || self.path.is_empty() {
            return;
        }
        match File::open(&self.path) {
            Ok(mut file) => {
                let _ = file.seek(SeekFrom::End(0));
                asr::print_message("Watching Client.txt");
                self.reader = Some(BufReader::new(file));
            }
            Err(_) => {
                // File not there (yet). Keep retrying on subsequent ticks.
            }
        }
    }

    /// Returns all complete lines appended since the last call, plus whether
    /// the file was truncated or recreated since then.
    fn poll_lines(&mut self) -> (Vec<String>, bool) {
        let mut lines = Vec::new();
        let mut truncated = false;

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
        // we already consumed, it was rewritten (new PoE2 session).
        if let Some(reader) = self.reader.as_mut() {
            if let (Ok(pos), Ok(meta)) = (reader.stream_position(), reader.get_ref().metadata()) {
                if meta.len() < pos {
                    truncated = true;
                    match File::open(&self.path) {
                        Ok(file) => *reader = BufReader::new(file),
                        Err(_) => self.reader = None,
                    }
                }
            }
        }

        (lines, truncated)
    }
}

// ---------------------------------------------------------------------------
// Log line parsing (ported from Component/ClientLog/ClientParser.cs)
// ---------------------------------------------------------------------------

enum Event<'a> {
    /// `... Generating level N area "<name>"` - the player entered an area.
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

    if let Some(name) = extract_area_name(line) {
        return Event::Area(name);
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
/// engine. The capture is everything between the quotes.
fn extract_area_name(line: &str) -> Option<&str> {
    let idx = line.find("Generating level ")?;
    let rest = &line[idx..];

    // `Generating level <N> area "<name>"` - the name spans from the first to
    // the last quote of the remainder.
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

async fn main() {
    let mut settings = Settings::register();
    asr::print_message("Path of Exile 2 AutoSplitter (WASM port) loaded");

    let mut tail = LogTail::new();
    let mut state = RunState::default();
    let mut prev_timer_state = timer::state();

    loop {
        settings.update();

        // React to the log path changing at runtime.
        if tail.sync_path(settings.client_log.path.as_str()) {
            state.clear();
        }
        tail.ensure_open();
        let (lines, truncated) = tail.poll_lines();
        if truncated {
            // New game session - reset progress tracking like a fresh start.
            state.clear();
        }

        // Mirror the original component's OnStart handler: clear all progress
        // whenever the timer transitions into Running or back to NotRunning.
        let current_timer_state = timer::state();
        let fresh_start =
            current_timer_state == TimerState::Running && prev_timer_state != TimerState::Running;
        let fresh_reset =
            current_timer_state == TimerState::NotRunning && prev_timer_state != TimerState::NotRunning;
        if fresh_start || fresh_reset {
            state.clear();
        }
        prev_timer_state = current_timer_state;

        for line in lines {
            handle_line(line, &settings, &mut state);
        }

        next_tick().await;
    }
}

fn handle_line(line: String, settings: &Settings, state: &mut RunState) {
    if !settings.auto_split {
        return;
    }

    match parse_line(&line) {
        Event::Area(area_name) => {
            // Unknown areas are ignored entirely.
            let Some(area) = AREAS.iter().find(|a| a.name == area_name) else {
                return;
            };

            // Automatically start the timer when entering The Riverbank.
            if area_name == START_AREA && settings.auto_start {
                if timer::state() == TimerState::NotRunning {
                    timer::start();
                }
                state.encountered_areas.insert(area_name.to_string());
                return;
            }

            // Only split if the active split set contains this area and we
            // haven't encountered it yet during this run.
            let in_active_set = match settings.split_mode {
                SplitMode::Full => true,
                SplitMode::AnyPercent => area.mandatory,
                SplitMode::Level => false,
            };

            if in_active_set && !state.encountered_areas.contains(area_name) {
                timer::split();
            }

            if area_name == END_AREA {
                // Additional Split() call to end the run, same as the original
                // component.
                timer::split();
            }

            state.encountered_areas.insert(area_name.to_string());
        }
        Event::Level(level) => {
            if settings.split_mode == SplitMode::Level
                && !state.levels_reached.contains(&level)
            {
                timer::split();
            }
            state.levels_reached.insert(level);
        }
        Event::InstanceDetails | Event::Nothing => {}
    }
}
