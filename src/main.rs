//! Notifications — a Trunk Recorder Pro plugin: rules over what the recorder
//! hears (a system's signals, tone patterns, emergencies, calls, new
//! talkgroups and radios), and where word of it goes: webhooks (JSON,
//! Discord, Slack, ntfy) and commands, with a call's recording to follow.

mod deliver;
mod payload;
mod rules;
mod seen;
mod settings;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use trunk_recorder_plugin::{
    topic, Action, CallInfo, ConcludedCall, Contribution, Host, Manifest, NoConfig, Outcome, Page, Plugin, Reply, Setup, TonePatternEvent, UnitEvent,
};

use payload::Notification;
use rules::{Notice, Notifier};
use seen::Seen;
use settings::{Destination, Event, Settings};

/// A follow-up waits this long for its call to be saved.
const FOLLOW_UP_WAIT: Duration = Duration::from_secs(3600);
/// Audio past this size isn't attached (Discord takes 10 MB without boosts).
const MAX_ATTACH: usize = 8 << 20;
/// What's been heard is saved this often (and at shutdown).
const SAVE_EVERY: Duration = Duration::from_secs(300);

pub fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// A notification whose call's recording is to follow it, to its rule's destinations as they were
/// when it fired (the rules may change before the call is saved).
struct FollowUp {
    n: Notification,
    to: Vec<String>,
    since: Instant,
}

struct Notify {
    host: Host,
    settings: Settings,
    notifier: Notifier,
    sender: deliver::Sender,
    seen: Seen,
    saved: Instant,
    follow_ups: Vec<FollowUp>,
    data_dir: PathBuf,
}

impl Plugin for Notify {
    type Config = Settings;
    type SystemConfig = NoConfig;

    fn manifest() -> Manifest {
        Manifest {
            name: "Notifications".into(),
            subscribe: vec![topic::TONE_PATTERN.into(), topic::CALL_START.into(), topic::UNIT.into(), topic::CALL_CONCLUDED.into()],
            page: Some(Page { title: "Notifications".into() }),
            live_settings: true,
            contribute: vec![Contribution {
                on: "signal".into(),
                label: "Notify".into(),
                hint: "Add a notification rule for this signal".into(),
                append: "rules".into(),
                item: json!({ "name": "{name}", "event": "signal", "signals": ["{signal}"] }),
            }],
            ..trunk_recorder_plugin::manifest!()
        }
    }

    fn start(host: Host, setup: Setup<Settings, NoConfig>) -> Result<Self, String> {
        let settings = setup.config;
        host.info(summary(&settings, Some(&host)));
        let sender = deliver::Sender::start(host.clone());
        let seen = Seen::load(&setup.data_dir);
        Ok(Notify { host, settings, notifier: Notifier::default(), sender, seen, saved: Instant::now(), follow_ups: Vec::new(), data_dir: setup.data_dir })
    }

    fn tone_pattern(&mut self, e: TonePatternEvent) {
        let p = &e.pattern;
        let on = on_tg(e.talkgroup, &e.alpha_tag);
        let n = Notice {
            event: Some(Event::Tone),
            system: e.short_name.clone(),
            site: e.site.clone(),
            talkgroup: Some(e.talkgroup),
            alpha_tag: e.alpha_tag.clone(),
            unit: (p.src > 0).then_some(p.src as u32),
            unit_tag: e.src_tag.clone(),
            pattern: p.pattern.clone(),
            tones: p.tones.clone(),
            digits: p.digits.clone(),
            knox: p.knox,
            frame: p.origin == "frame",
            text: format!("{} {on}", p.text),
            call_num: Some(e.call_num),
            time: e.time,
            detail: serde_json::to_value(&e).unwrap_or_default(),
            ..Default::default()
        };
        if !p.signals.is_empty() {
            let named = p.signals.iter().map(|s| format!("{}:{s}", e.short_name)).collect::<Vec<_>>().join(", ");
            self.fire(Notice { event: Some(Event::Signal), signals: p.signals.clone(), text: format!("{named} {on}"), ..n.clone() });
        }
        self.fire(n);
    }

    fn call_start(&mut self, c: CallInfo) {
        let unit = c.sources.last().map(|s| s.unit).or(c.units.last().copied()).filter(|&u| u != 0);
        let tag_of = |u: u32| c.sources.iter().find(|s| s.unit == u).map_or(String::new(), |s| s.tag.clone());
        let unit_tag = unit.map_or(String::new(), tag_of);
        let who = unit.map_or(String::new(), |u| format!(", unit {}", named(u, &unit_tag)));
        let n = Notice {
            event: Some(Event::Call),
            system: c.short_name.clone(),
            site: c.site.clone(),
            talkgroup: Some(c.talkgroup),
            alpha_tag: c.talkgroup_tag.clone(),
            unit,
            unit_tag,
            text: format!("Call {}{who}", on_tg(c.talkgroup, &c.talkgroup_tag)),
            call_num: Some(c.id as u64),
            encrypted: Some(c.encrypted),
            time: c.start_time,
            detail: json!({ "freq": c.freq_hz, "encrypted": c.encrypted, "recording": c.recording }),
            ..Default::default()
        };
        if c.emergency {
            self.fire(Notice { event: Some(Event::Emergency), text: format!("Emergency: {}", n.text), ..n.clone() });
        }
        // First heard: the talkgroup, and each radio on it.
        let units: Vec<u32> = c.sources.iter().map(|s| s.unit).chain(c.units.iter().copied()).collect();
        let firsts = self.seen.heard(&c.short_name, c.talkgroup, &units, now());
        if firsts.talkgroup {
            let alpha = if c.talkgroup_tag.is_empty() { String::new() } else { format!(" {}", c.talkgroup_tag) };
            self.fire(Notice {
                event: Some(Event::NewTalkgroup),
                unit: None,
                unit_tag: String::new(),
                text: format!("New talkgroup: {}{alpha}", c.talkgroup),
                ..n.clone()
            });
        }
        for u in firsts.units {
            let tag = tag_of(u);
            let text = format!("New radio: {} {}", named(u, &tag), on_tg(c.talkgroup, &c.talkgroup_tag));
            self.fire(Notice { event: Some(Event::NewRadio), unit: Some(u), unit_tag: tag, text, ..n.clone() });
        }
        self.fire(n);
        if self.saved.elapsed() > SAVE_EVERY {
            self.save();
        }
    }

    fn unit(&mut self, u: UnitEvent) {
        if u.kind != "emergency_alarm" || u.unit == 0 {
            return;
        }
        let alpha = u.alpha_tag.clone();
        self.fire(Notice {
            event: Some(Event::Emergency),
            system: u.short_name.clone(),
            site: u.site.clone(),
            talkgroup: u.talkgroup,
            alpha_tag: alpha.clone(),
            unit: Some(u.unit),
            unit_tag: u.tag.clone(),
            text: format!("Emergency alarm: unit {}{}", named(u.unit, &u.tag), u.talkgroup.map_or(String::new(), |tg| format!(" {}", on_tg(tg, &alpha)))),
            time: u.time,
            detail: json!({ "kind": "emergency_alarm" }),
            ..Default::default()
        });
    }

    fn call_concluded(&mut self, c: ConcludedCall) {
        let r = &c.call;
        // Notifications about this call: its system and talkgroup, a moment it was on.
        let ours = |f: &FollowUp| {
            f.n.short_name == r.short_name
                && f.n.talkgroup == Some(r.talkgroup)
                && f.n.time >= r.start_time as f64 - 2.0
                && f.n.time <= r.stop_time as f64 + 2.0
        };
        let (due, keep): (Vec<FollowUp>, Vec<FollowUp>) = std::mem::take(&mut self.follow_ups).into_iter().partition(ours);
        self.follow_ups = keep.into_iter().filter(|f| f.since.elapsed() < FOLLOW_UP_WAIT).collect();
        if due.is_empty() {
            self.host.call_result(&c.path, Outcome::Skipped, "", "");
            return;
        }
        // Decrypted voice goes no further than the recorder was let send it: no audio, no link.
        let (bytes, ext) = match c.files.m4a.as_ref().and_then(|p| std::fs::read(p).ok()) {
            Some(b) => (b, "m4a"),
            None => (std::fs::read(&c.files.wav).unwrap_or_default(), "wav"),
        };
        let name = format!("{}.{ext}", c.path.rsplit('/').next().unwrap_or("call"));
        let audio = (!r.decrypted && !bytes.is_empty() && bytes.len() <= MAX_ATTACH).then(|| Arc::new((bytes, name)));
        let base = self.settings.recorder_address.trim().trim_end_matches('/');
        let url = if base.is_empty() || r.decrypted { String::new() } else { format!("{base}/calls/{}.{ext}", c.path) };
        let secs = r.call_length_ms as f64 / 1000.0;
        // Once to each rule's destinations, however many of its notifications asked (a page and its warble).
        let mut sent: Vec<String> = Vec::new();
        for f in due {
            if sent.contains(&f.n.rule) {
                continue;
            }
            sent.push(f.n.rule.clone());
            let mut n = f.n;
            n.id = self.notifier.next_id();
            n.event = "recording".into();
            n.text = format!("{} (recording, {secs:.0} s)", n.text);
            n.url = url.clone();
            n.call_num = Some(r.call_num);
            n.detail = json!({ "path": c.path, "length": secs });
            self.tell(n, &f.to, audio.clone());
        }
        self.host.call_result(&c.path, Outcome::Ok, "recording sent", "");
    }

    fn action(&mut self, a: Action, reply: Reply) {
        let recording = match a.action.as_str() {
            "test" => false,
            "test_recording" => true,
            other => return reply.fail(format!("No action {other}")),
        };
        let dest: Destination = match serde_json::from_value(a.value) {
            Ok(d) => d,
            Err(e) => return reply.fail(format!("Not a destination: {e}")),
        };
        // (The recorder's messages keep coming meanwhile.)
        std::thread::spawn(move || match deliver::test(&dest, recording) {
            Ok(()) => reply.ok("Sent"),
            Err(e) => reply.fail(e),
        });
    }

    // Edited on its page: taken as they come (rules' cooldowns, follow-ups and what's been heard carry on).
    fn settings(&mut self, setup: Setup<Settings, NoConfig>) -> Result<(), String> {
        // (No warnings here: a name half typed in the form would be one.)
        self.host.debug(summary(&setup.config, None));
        self.settings = setup.config;
        Ok(())
    }

    fn shutdown(&mut self, _grace: Duration) {
        self.save();
    }
}

impl Notify {
    /// Every rule `n` fires, told.
    fn fire(&mut self, n: Notice) {
        if !self.settings.rules.iter().any(|r| r.enabled && Some(r.event) == n.event) {
            return;
        }
        for (i, note) in self.notifier.notice(&self.settings.rules, &n) {
            let rule = &self.settings.rules[i];
            if rule.follow_up {
                self.follow_ups.push(FollowUp { n: note.clone(), to: rule.to.clone(), since: Instant::now() });
            }
            let to = rule.to.clone();
            self.tell(note, &to, None);
        }
    }

    /// A notification to its destinations (and the log, which keeps what was sent).
    fn tell(&mut self, n: Notification, to: &[String], audio: Option<deliver::Audio>) {
        self.host.info(format!("{}: {}", n.title, n.text));
        for d in to.iter().filter_map(|name| self.settings.destinations.iter().find(|d| &d.name == name)) {
            self.sender.send(n.clone(), d.clone(), audio.clone());
        }
    }

    fn save(&mut self) {
        if let Err(e) = self.seen.save() {
            self.host.warn(format!("couldn't save what's been heard to {}: {e}", self.data_dir.display()));
        }
        self.saved = Instant::now();
    }
}

/// "3 rules on, 1 destination", having warned (`host`) of rules sending to destinations there aren't.
fn summary(s: &Settings, host: Option<&Host>) -> String {
    let names: Vec<&str> = s.destinations.iter().map(|d| d.name.as_str()).collect();
    for r in &s.rules {
        for to in r.to.iter().filter(|to| !names.contains(&to.as_str())) {
            if let Some(h) = host {
                h.warn(format!("Rule {}: sends to {to}, which isn't one of the destinations", r.name));
            }
        }
    }
    let on = s.rules.iter().filter(|r| r.enabled).count();
    format!("{on} rule{} on, {} destination{}", if on == 1 { "" } else { "s" }, names.len(), if names.len() == 1 { "" } else { "s" })
}

/// "1675303 (Engine 4)", or the number alone.
fn named(unit: u32, tag: &str) -> String {
    if tag.is_empty() {
        unit.to_string()
    } else {
        format!("{unit} ({tag})")
    }
}

/// "on TG 401 Fire Dispatch".
fn on_tg(tg: u32, alpha: &str) -> String {
    if alpha.is_empty() {
        format!("on TG {tg}")
    } else {
        format!("on TG {tg} {alpha}")
    }
}

fn main() {
    trunk_recorder_plugin::run::<Notify>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use trunk_recorder_plugin::{testing, HostMessage, TonePattern};

    fn tone(signals: &[&str]) -> TonePatternEvent {
        TonePatternEvent {
            short_name: "wood".into(),
            talkgroup: 1,
            alpha_tag: "Fire Dispatch".into(),
            time: now(),
            pattern: TonePattern {
                pattern: "two_tone".into(),
                tones: vec![467.1, 1032.3],
                text: "two-tone 467.1 → 1032.3 Hz".into(),
                signals: signals.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// A signal heard: its rule fires (to a command destination, which writes what it got).
    #[cfg(unix)]
    #[test]
    fn a_signal_fires_its_rule() {
        let dir = testing::temp_dir("notify");
        let out = dir.join("got.json");
        let config = json!({
            "rules": [
                { "name": "Fire page", "event": "signal", "signals": ["wood:FD_PAGE"], "title": "LOCALTOWN FIRE PAGE", "to": ["log"] },
                { "name": "Any tone", "event": "tone", "patterns": ["morse"], "to": ["log"] }
            ],
            "destinations": [{ "name": "log", "type": "command", "command": format!("cat > {}", out.display()) }]
        });
        let hello = testing::hello(&dir, config);
        let run = testing::run::<Notify>([HostMessage::Hello(hello), HostMessage::TonePattern(tone(&["FD_PAGE"])), HostMessage::TonePattern(tone(&[]))]);
        assert!(run.ready());
        // (The sending thread may still be at it.)
        for _ in 0..50 {
            if out.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let got: serde_json::Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
        assert_eq!(
            (got["title"].as_str(), got["event"].as_str(), got["signals"][0].as_str()),
            (Some("LOCALTOWN FIRE PAGE"), Some("signal"), Some("wood:FD_PAGE"))
        );
        assert!(run.logs().iter().any(|(_, l)| l.contains("LOCALTOWN FIRE PAGE: wood:FD_PAGE on TG 1 Fire Dispatch")), "{:?}", run.logs());
        assert!(!run.logs().iter().any(|(_, l)| l.contains("Any tone")), "not Morse");
    }

    #[test]
    fn the_manifest_offers_a_page_and_a_button() {
        let m = trunk_recorder_plugin::describe::<Notify>();
        assert_eq!(m.page.as_ref().map(|p| p.title.as_str()), Some("Notifications"));
        assert_eq!(m.contribute[0].append, "rules");
        let schema = m.config.unwrap();
        assert_eq!(schema["properties"]["destinations"]["items"]["x-actions"][0]["id"], "test");
        assert_eq!(schema["properties"]["rules"]["items"]["properties"]["to"]["x-ref"], "destinations");
    }
}
