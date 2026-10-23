//! The plugin's settings: rules, and the destinations they send to. The
//! recorder draws the form from this schema (on the plugin's own page).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use trunk_recorder_plugin::filter::patterns;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Rules
    ///
    /// What to watch for, and where to send word. Every condition a rule sets has to hold; one left
    /// empty is any. For "this or that", make two rules. A signal's Notify button (on its system's
    /// Tones card) adds a rule for it.
    pub rules: Vec<Rule>,
    /// Destinations
    ///
    /// Where rules send word: a webhook, or a command on this computer.
    pub destinations: Vec<Destination>,
    /// Link recordings from
    ///
    /// A recording goes as a file to Discord and ntfy, inside the JSON of JSON webhooks, and to commands
    /// (TRP_AUDIO). Slack's webhooks can't take a file, so they need a link: this recorder's address as
    /// the people reading the message reach it (http://100.64.0.5:8091). Every recording then has a link
    /// too. Empty: no links. A link plays the call while Setup's Recording settings keep its audio.
    #[schemars(extend("format" = "uri"))]
    pub recorder_address: String,
}

/// Rule
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
#[schemars(extend(
    "x-title" = "name",
    "x-summary" = ["{event}", "{signals|any signal}", "{tones}", "{patterns}", "{systems|any system}", "TG {talkgroups}", "radio {units}", "{followUp}", "→ {to|nowhere yet}"]
))]
pub struct Rule {
    /// Name
    #[schemars(extend("x-required" = true))]
    pub name: String,
    /// On
    pub enabled: bool,
    /// When
    pub event: Event,
    /// Systems
    ///
    /// Empty: any.
    #[schemars(extend("x-systems" = true))]
    pub systems: Vec<String>,
    /// Signals
    ///
    /// Which (empty: any signal of the systems'). Signals are named on each system's Tones card.
    #[schemars(extend("x-signals" = true))]
    #[schemars(extend("x-when" = { "event": "signal" }))]
    pub signals: Vec<String>,
    /// Kinds of tone
    ///
    /// Empty: any.
    #[schemars(extend("x-when" = { "event": "tone" }))]
    pub patterns: Vec<Pattern>,
    /// Tones
    ///
    /// Particular tones, typed in: a page from a paging list that hasn't been heard yet, say. Empty:
    /// any tone of the kinds above. (Tones heard and named are signals: "A signal is heard".)
    #[schemars(extend("x-tones" = true, "x-when" = { "event": "tone" }))]
    pub tones: Vec<ToneSpec>,
    /// Talkgroups
    ///
    /// Numbers, ranges (1000-1099) and patterns (507*). Empty: any.
    #[serde(deserialize_with = "patterns")]
    pub talkgroups: Vec<String>,
    /// Radios
    ///
    /// Radio IDs: numbers, ranges and patterns. Empty: any.
    #[serde(deserialize_with = "patterns")]
    pub units: Vec<String>,
    /// Encryption
    #[schemars(extend("x-when" = { "event": ["call", "emergency"] }))]
    pub encrypted: Encryption,
    /// Not again within, s
    ///
    /// It fires once for the same system, talkgroup and signal (or pattern) in this long.
    #[schemars(range(min = 0, max = 86400))]
    pub cooldown_s: u32,
    /// Title
    ///
    /// What leads ("LOCALTOWN FIRE PAGE"). Empty: the rule's name. {system}, {talkgroup}, {alpha_tag},
    /// {unit}, {unit_tag}, {signals}, {site} and {text} are filled in.
    pub title: String,
    /// Message
    ///
    /// The words. Empty: what happened ({text}). The same placeholders as the title.
    #[schemars(extend("x-multiline" = true))]
    pub message: String,
    /// Then the recording
    ///
    /// Once the call is saved, a second notification with its audio (see "Link recordings from").
    pub follow_up: bool,
    /// Send to
    #[schemars(extend("x-ref" = "destinations"))]
    pub to: Vec<String>,
}

impl Default for Rule {
    fn default() -> Self {
        Rule {
            name: String::new(),
            enabled: true,
            event: Event::Signal,
            systems: Vec::new(),
            signals: Vec::new(),
            patterns: Vec::new(),
            tones: Vec::new(),
            talkgroups: Vec::new(),
            units: Vec::new(),
            encrypted: Encryption::Either,
            cooldown_s: 60,
            title: String::new(),
            message: String::new(),
            follow_up: false,
            to: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    /// A signal is heard
    #[default]
    Signal,
    /// A tone is heard
    Tone,
    /// An emergency (a call, or a radio's alarm)
    Emergency,
    /// A call begins
    Call,
    /// A new talkgroup is heard
    NewTalkgroup,
    /// A new radio is heard
    NewRadio,
}

impl Event {
    pub fn name(self) -> &'static str {
        match self {
            Event::Signal => "signal",
            Event::Tone => "tone",
            Event::Emergency => "emergency",
            Event::Call => "call",
            Event::NewTalkgroup => "new_talkgroup",
            Event::NewRadio => "new_radio",
        }
    }
}

/// A tone typed in, as a system's signal has it (its pattern, tones or digits; its lengths are for
/// playing it, not matched).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct ToneSpec {
    pub pattern: Pattern,
    /// Hz, in order (two-tone: A then B).
    pub tones_hz: Vec<f64>,
    /// Digits (`KNOX ` first for KNOX tones); Morse: the text.
    pub digits: String,
    pub lengths_s: Vec<f64>,
    /// How much further than usual ([`tolerance`]) a heard tone may be, % of it.
    pub extra_tolerance_pct: f64,
}

impl Default for ToneSpec {
    fn default() -> Self {
        ToneSpec { pattern: Pattern::TwoTone, tones_hz: Vec::new(), digits: String::new(), lengths_s: Vec::new(), extra_tolerance_pct: 0.0 }
    }
}

/// How far a heard tone may be from `hz` and still be it, Hz: as the recorder matches signals.
/// Measured in audio, 2.5 %; from Phase 2's tone frames, which name a tone on a 31.25 Hz grid,
/// 15.7 Hz and 0.5 %.
pub fn tolerance(hz: f64, from_frame: bool) -> f64 {
    if from_frame {
        15.7 + 0.005 * hz
    } else {
        0.025 * hz
    }
}

/// A tone pattern, as the call JSON's `tone_patterns` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    /// Two-tone page
    TwoTone,
    /// Long tone
    Long,
    /// Hi-low warble
    HiLow,
    /// Pulsed tone
    Pulsed,
    /// DTMF digits
    Digits,
    /// KNOX digits
    Knox,
    /// Morse ID
    Morse,
    /// Tone sequence
    Sequence,
    /// Beep
    Beep,
}

impl Pattern {
    pub fn name(self) -> &'static str {
        match self {
            Pattern::TwoTone => "two_tone",
            Pattern::Long => "long",
            Pattern::HiLow => "hi_low",
            Pattern::Pulsed => "pulsed",
            Pattern::Digits => "digits",
            Pattern::Knox => "knox",
            Pattern::Morse => "morse",
            Pattern::Sequence => "sequence",
            Pattern::Beep => "beep",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Encryption {
    /// Either
    #[default]
    Either,
    /// Encrypted only
    Encrypted,
    /// Clear only
    Clear,
}

/// Destination
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
#[schemars(extend(
    "x-title" = "name",
    "x-summary" = ["{type}", "{format}", "{command}"],
    "x-actions" = [
        { "id": "test", "label": "Send a test", "hint": "Sends a test notification now, as it's set up here" },
        { "id": "test_recording", "label": "Send a test recording", "hint": "As a call's recording follows: a few seconds of a page (Discord: as a file)" }
    ]
))]
pub struct Destination {
    /// Name
    ///
    /// What rules send to it by.
    #[schemars(extend("x-required" = true))]
    pub name: String,
    /// Kind
    #[serde(rename = "type")]
    pub kind: Kind,
    /// Format
    ///
    /// What's sent. JSON is the notification itself.
    #[schemars(extend("x-when" = { "type": "webhook" }))]
    pub format: Format,
    /// URL
    ///
    /// Where to. A Discord or Slack webhook's address, an ntfy topic's
    /// (https://ntfy.sh/station4), an Apprise API's.
    #[schemars(extend("x-secret" = true))]
    #[schemars(extend("x-when" = { "type": "webhook" }))]
    pub url: String,
    /// Method
    ///
    /// Empty for POST.
    #[schemars(extend("x-when" = { "type": "webhook" }))]
    pub method: String,
    /// Headers
    ///
    /// One a line, as "Authorization: Bearer abc".
    #[schemars(extend("x-multiline" = true))]
    #[schemars(extend("x-when" = { "type": "webhook" }))]
    pub headers: String,
    /// Own body
    ///
    /// A template in place of the format. {title}, {text}, {rule}, {event}, {system},
    /// {talkgroup}, {alpha_tag}, {unit}, {signals}, {url} and {json} (the notification) are filled
    /// in. One that begins { is sent as JSON, its values escaped.
    #[schemars(extend("x-multiline" = true))]
    #[schemars(extend("x-when" = { "type": "webhook" }))]
    pub body: String,
    /// Command
    ///
    /// Run by the shell as the recorder's user, given the notification's JSON on stdin and
    /// TRP_TITLE, TRP_TEXT, TRP_RULE, TRP_EVENT, TRP_SYSTEM, TRP_TALKGROUP, TRP_UNIT, TRP_SIGNALS and
    /// TRP_URL. It has 30 seconds.
    #[schemars(extend("x-when" = { "type": "command" }))]
    pub command: String,
}

impl Destination {
    /// The headers it's given ("Name: value" lines).
    pub fn header_list(&self) -> Vec<(String, String)> {
        self.headers
            .lines()
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .filter(|(k, _)| !k.is_empty())
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Webhook
    #[default]
    Webhook,
    /// Command
    Command,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// JSON
    #[default]
    Json,
    /// Discord
    Discord,
    /// Slack
    Slack,
    /// ntfy
    Ntfy,
}
