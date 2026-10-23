//! Rules over what's heard: a notice (a signal, a tone pattern, an
//! emergency, a call, a first) fires each enabled rule whose conditions all
//! hold and that hasn't fired for the same thing within its cooldown.

use std::collections::HashMap;

use serde_json::Value;
use trunk_recorder_plugin::filter::glob;

use crate::payload::Notification;
use crate::settings::{Encryption, Event, Rule};

/// Something heard, as rules see it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notice {
    pub event: Option<Event>,
    pub system: String,
    pub site: String,
    pub talkgroup: Option<u32>,
    pub alpha_tag: String,
    pub unit: Option<u32>,
    pub unit_tag: String,
    /// Signals: their ids (without the system).
    pub signals: Vec<String>,
    /// Tones and signals: the pattern (`two_tone`, …).
    pub pattern: String,
    pub text: String,
    pub call_num: Option<u64>,
    /// Calls: whether it's encrypted (as far as is known when it begins).
    pub encrypted: Option<bool>,
    /// Unix s.
    pub time: f64,
    pub detail: Value,
}

/// A list of numbers, ranges (`1000-1099`) and patterns (`507*`) names `v` (an empty one, anything).
fn listed(list: &[String], v: Option<u32>) -> bool {
    let names = |entry: &String, v: u32| match entry.trim().split_once('-').map(|(a, b)| (a.trim().parse::<u32>(), b.trim().parse::<u32>())) {
        Some((Ok(a), Ok(b))) => (a..=b).contains(&v),
        _ => glob(entry.trim(), &v.to_string()),
    };
    list.is_empty() || v.is_some_and(|v| list.iter().any(|e| names(e, v)))
}

/// A rule's signal (`system:ID`, or a bare `ID` for any system's) is one heard on `system`.
fn signal_is(want: &str, system: &str, heard: &str) -> bool {
    match want.split_once(':') {
        Some((s, id)) => s.eq_ignore_ascii_case(system) && id == heard,
        None => want == heard,
    }
}

impl Notice {
    /// `system:ID` for each of its signals.
    pub fn signal_names(&self) -> Vec<String> {
        self.signals.iter().map(|s| format!("{}:{s}", self.system)).collect()
    }
}

/// Whether `n` is what rule `r` asks for.
pub fn matches(r: &Rule, n: &Notice) -> bool {
    r.enabled
        && n.event == Some(r.event)
        && (r.systems.is_empty() || r.systems.iter().any(|s| s.eq_ignore_ascii_case(&n.system)))
        && listed(&r.talkgroups, n.talkgroup)
        && listed(&r.units, n.unit)
        && (r.signals.is_empty() || n.signals.iter().any(|h| r.signals.iter().any(|w| signal_is(w, &n.system, h))))
        && (r.patterns.is_empty() || r.patterns.iter().any(|p| p.name() == n.pattern))
        && match r.encrypted {
            Encryption::Either => true,
            Encryption::Encrypted => n.encrypted == Some(true),
            Encryption::Clear => n.encrypted == Some(false),
        }
}

/// The rules' state: when each last fired, for what.
#[derive(Default)]
pub struct Notifier {
    next_id: u64,
    fired: HashMap<String, f64>,
}

impl Notifier {
    pub fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// The rules `n` fires (each at most once in its cooldown for the same
    /// thing), as notifications, with each rule's place in `rules`.
    pub fn notice(&mut self, rules: &[Rule], n: &Notice) -> Vec<(usize, Notification)> {
        let mut out = Vec::new();
        for (i, r) in rules.iter().enumerate().filter(|(_, r)| matches(r, n)) {
            let what = format!("{}|{}|{:?}|{}|{}", r.name, n.system, n.talkgroup, n.signals.join(","), n.pattern);
            if self.fired.get(&what).is_some_and(|&t| n.time - t < r.cooldown_s as f64) {
                continue;
            }
            self.fired.insert(what, n.time);
            out.push((i, notification(self.next_id(), r, n)));
        }
        // (What's past every cooldown is forgotten.)
        if self.fired.len() > 1000 {
            let longest = rules.iter().map(|r| r.cooldown_s).max().unwrap_or(0) as f64;
            self.fired.retain(|_, &mut t| n.time - t < longest);
        }
        out
    }
}

/// A rule's template filled in from what was heard (plain words, no escaping).
fn fill(template: &str, r: &Rule, n: &Notice) -> String {
    let opt = |v: Option<u32>| v.map_or(String::new(), |v| v.to_string());
    [
        ("{text}", n.text.clone()),
        ("{rule}", r.name.clone()),
        ("{event}", n.event.map_or("", Event::name).to_string()),
        ("{system}", n.system.clone()),
        ("{site}", n.site.clone()),
        ("{talkgroup}", opt(n.talkgroup)),
        ("{alpha_tag}", n.alpha_tag.clone()),
        ("{unit}", opt(n.unit)),
        ("{unit_tag}", n.unit_tag.clone()),
        ("{signals}", n.signal_names().join(", ")),
    ]
    .iter()
    .fold(template.to_string(), |s, (k, v)| s.replace(k, v))
}

fn notification(id: u64, r: &Rule, n: &Notice) -> Notification {
    Notification {
        id,
        rule: r.name.clone(),
        event: n.event.map_or("", Event::name).to_string(),
        title: if r.title.trim().is_empty() { r.name.clone() } else { fill(&r.title, r, n) },
        short_name: n.system.clone(),
        site: n.site.clone(),
        talkgroup: n.talkgroup,
        alpha_tag: n.alpha_tag.clone(),
        unit: n.unit,
        unit_tag: n.unit_tag.clone(),
        signals: n.signal_names(),
        text: if r.message.trim().is_empty() { n.text.clone() } else { fill(&r.message, r, n) },
        call_num: n.call_num,
        url: String::new(),
        time: n.time,
        detail: n.detail.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rule(v: Value) -> Rule {
        serde_json::from_value(v).unwrap()
    }

    fn heard(system: &str, tg: u32, signals: &[&str], t: f64) -> Notice {
        Notice {
            event: Some(Event::Signal),
            system: system.into(),
            talkgroup: Some(tg),
            signals: signals.iter().map(|s| s.to_string()).collect(),
            pattern: "two_tone".into(),
            text: "page".into(),
            time: t,
            ..Default::default()
        }
    }

    #[test]
    fn rules_narrow_by_each_condition() {
        let r = rule(json!({ "name": "csp", "systems": ["CLMRN"], "talkgroups": ["401"], "signals": ["CLMRN:CSP_FIRE"], "to": ["hook"] }));
        assert!(matches(&r, &heard("CLMRN", 401, &["CSP_FIRE"], 0.0)));
        assert!(matches(&r, &heard("clmrn", 401, &["OTHER", "CSP_FIRE"], 0.0)), "the system's case; any of its signals");
        assert!(!matches(&r, &heard("wood", 401, &["CSP_FIRE"], 0.0)));
        assert!(!matches(&r, &heard("CLMRN", 402, &["CSP_FIRE"], 0.0)));
        assert!(!matches(&r, &heard("CLMRN", 401, &["OTHER"], 0.0)));
        // A signal named without its system: any system's.
        assert!(matches(&rule(json!({ "name": "any", "signals": ["CSP_FIRE"] })), &heard("wood", 1, &["CSP_FIRE"], 0.0)));
        // Empty conditions: any.
        assert!(matches(&rule(json!({ "name": "wood", "systems": ["wood"] })), &heard("wood", 3, &["SIREN_SIGNAL_1"], 0.0)));
        assert!(!matches(&rule(json!({ "name": "x", "enabled": false })), &heard("wood", 3, &["S"], 0.0)));
        // Another event.
        let em = rule(json!({ "name": "em", "event": "emergency", "units": ["1670000-1679999"] }));
        let alarm = Notice { event: Some(Event::Emergency), system: "wood".into(), unit: Some(1675303), ..Default::default() };
        assert!(matches(&em, &alarm) && !matches(&em, &Notice { unit: Some(42), ..alarm.clone() }));
        // Patterns.
        let pages = rule(json!({ "name": "p", "event": "tone", "patterns": ["two_tone", "hi_low"] }));
        let tone = Notice { event: Some(Event::Tone), pattern: "hi_low".into(), ..Default::default() };
        assert!(matches(&pages, &tone) && !matches(&pages, &Notice { pattern: "morse".into(), ..tone.clone() }));
    }

    #[test]
    fn a_rule_fires_once_in_its_cooldown() {
        let rules = [rule(json!({ "name": "csp", "signals": ["CSP_FIRE"], "cooldownS": 60, "to": ["hook"] }))];
        let mut n = Notifier::default();
        let fired = n.notice(&rules, &heard("CLMRN", 401, &["CSP_FIRE"], 100.0));
        assert_eq!(fired.len(), 1);
        assert_eq!((fired[0].0, fired[0].1.rule.as_str(), fired[0].1.id), (0, "csp", 1));
        assert!(n.notice(&rules, &heard("CLMRN", 401, &["CSP_FIRE"], 130.0)).is_empty(), "within the cooldown");
        assert_eq!(n.notice(&rules, &heard("CLMRN", 402, &["CSP_FIRE"], 130.0)).len(), 1, "another talkgroup is another thing");
        assert_eq!(n.notice(&rules, &heard("CLMRN", 401, &["CSP_FIRE"], 161.0)).len(), 1, "after it");
    }

    #[test]
    fn titles_messages_and_encryption() {
        let r = rule(json!({ "name": "Fire", "event": "signal", "title": "LOCALTOWN FIRE PAGE", "message": "{signals} on {alpha_tag}: {text}" }));
        let mut n = Notifier::default();
        let heard = Notice { alpha_tag: "Fire Dispatch".into(), text: "two-tone 467.1 → 1032.3 Hz".into(), ..heard("wood", 1, &["FD_PAGE"], 0.0) };
        let (_, f) = n.notice(&[r], &heard).remove(0);
        assert_eq!(
            (f.title.as_str(), f.text.as_str(), f.rule.as_str()),
            ("LOCALTOWN FIRE PAGE", "wood:FD_PAGE on Fire Dispatch: two-tone 467.1 → 1032.3 Hz", "Fire")
        );
        assert_eq!(f.signals, ["wood:FD_PAGE"]);
        // Clear calls only.
        let clear = rule(json!({ "name": "c", "event": "call", "encrypted": "clear" }));
        let call = Notice { event: Some(Event::Call), encrypted: Some(false), ..Default::default() };
        assert!(matches(&clear, &call) && !matches(&clear, &Notice { encrypted: Some(true), ..call.clone() }));
    }
}
