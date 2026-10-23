//! A notification, and what each kind of destination is sent for it: the
//! JSON itself, a Discord embed, Slack blocks, ntfy's words and headers, or
//! the destination's own template.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::settings::{Destination, Format};

/// A rule fired: what happened. Webhooks in the JSON format get it as it is.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notification {
    /// Unique while the plugin runs.
    pub id: u64,
    /// The rule's name.
    pub rule: String,
    /// `signal`, `tone`, `emergency`, `call`, `new_talkgroup`, `new_radio`,
    /// `recording` (a call's recording, following a notification about it)
    /// or `test`.
    pub event: String,
    /// The headline: the rule's own title, or its name.
    pub title: String,
    pub short_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub site: String,
    pub talkgroup: Option<u32>,
    pub alpha_tag: String,
    pub unit: Option<u32>,
    pub unit_tag: String,
    /// The signals heard, as `system:ID`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<String>,
    /// For a person to read: "wood:FD_PAGE on TG 1 Fire Dispatch".
    pub text: String,
    pub call_num: Option<u64>,
    /// `recording`: where to play it (when the recorder's address is set).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// Unix seconds.
    pub time: f64,
    /// What the event said besides: a tone pattern's `pattern`, `tones`, `lengths`, `format`, ….
    pub detail: Value,
}

/// A test notification, for trying a destination.
pub fn test_notification() -> Notification {
    Notification {
        rule: "Test".into(),
        title: "Test".into(),
        event: "test".into(),
        text: "A test notification from Trunk Recorder Pro".into(),
        time: crate::now(),
        detail: json!({}),
        ..Default::default()
    }
}

/// Whether a template is JSON (it begins `{` or `[`): sent as JSON, its values escaped.
pub fn is_json(template: &str) -> bool {
    let t = template.trim_start();
    t.starts_with('{') || t.starts_with('[')
}

/// A webhook's own body: `template` filled in (in a JSON template, each value
/// as a JSON string's insides); empty, the notification's JSON.
pub fn render(template: &str, n: &Notification) -> String {
    let json = serde_json::to_string(n).unwrap_or_default();
    if template.trim().is_empty() {
        return json;
    }
    let escape = is_json(template);
    let value = |v: String| if escape { serde_json::to_string(&v).map(|q| q[1..q.len() - 1].to_string()).unwrap_or_default() } else { v };
    let opt = |v: Option<u32>| v.map_or(String::new(), |v| v.to_string());
    [
        ("{text}", n.text.clone()),
        ("{title}", n.title.clone()),
        ("{rule}", n.rule.clone()),
        ("{event}", n.event.clone()),
        ("{url}", n.url.clone()),
        ("{system}", n.short_name.clone()),
        ("{talkgroup}", opt(n.talkgroup)),
        ("{alpha_tag}", n.alpha_tag.clone()),
        ("{unit}", opt(n.unit)),
        ("{signals}", n.signals.join(", ")),
    ]
    .into_iter()
    .map(|(k, v)| (k, value(v)))
    .chain([("{json}", json)])
    .fold(template.to_string(), |s, (k, v)| s.replace(k, &v))
}

/// A command's environment: `TRP_TEXT`, `TRP_RULE`, ….
pub fn command_env(n: &Notification) -> Vec<(&'static str, String)> {
    let opt = |v: Option<u32>| v.map_or(String::new(), |v| v.to_string());
    vec![
        ("TRP_TITLE", n.title.clone()),
        ("TRP_URL", n.url.clone()),
        ("TRP_TEXT", n.text.clone()),
        ("TRP_RULE", n.rule.clone()),
        ("TRP_EVENT", n.event.clone()),
        ("TRP_SYSTEM", n.short_name.clone()),
        ("TRP_TALKGROUP", opt(n.talkgroup)),
        ("TRP_UNIT", opt(n.unit)),
        ("TRP_SIGNALS", n.signals.join(",")),
    ]
}

/// A recording's audio, and its file name.
pub type Audio = (Vec<u8>, String);

/// What a webhook sends.
#[derive(Clone, Debug, PartialEq)]
pub struct Payload {
    pub content_type: String,
    pub body: Vec<u8>,
    /// Added to the URL's query (ntfy takes its title and the rest there,
    /// where they can be any text).
    pub query: Vec<(String, String)>,
    /// Discord: the recording's file goes with it (multipart), the body as `payload_json`.
    pub multipart: bool,
}

/// The audio's type, by its name.
pub fn audio_type(name: &str) -> &'static str {
    if name.ends_with(".m4a") {
        "audio/mp4"
    } else {
        "audio/wav"
    }
}

/// What `d` sends for `n`: its own body if it has one, else its format's. A
/// recording's `audio` goes with it where the format can carry a file:
/// Discord's attached, ntfy's as the attachment, JSON's inline (base64);
/// Slack's incoming webhooks can't, and get the link alone.
pub fn payload(d: &Destination, n: &Notification, audio: Option<&Audio>) -> Payload {
    let audio = audio.filter(|_| n.event == "recording");
    let json =
        |v: Value| Payload { content_type: "application/json".into(), body: serde_json::to_vec(&v).unwrap_or_default(), query: Vec::new(), multipart: false };
    if !d.body.trim().is_empty() {
        let ct = if is_json(&d.body) { "application/json" } else { "text/plain; charset=utf-8" };
        return Payload { content_type: ct.into(), body: render(&d.body, n).into_bytes(), query: Vec::new(), multipart: false };
    }
    match d.format {
        Format::Discord => Payload { multipart: audio.is_some(), ..json(discord(n)) },
        Format::Slack => json(slack(n)),
        Format::Ntfy => {
            let mut query = vec![("title".to_string(), n.title.clone()), ("tags".to_string(), tag(&n.event).to_string())];
            if matches!(n.event.as_str(), "emergency" | "signal") {
                query.push(("priority".into(), "high".into()));
            }
            if !n.url.is_empty() {
                query.push(("click".into(), n.url.clone()));
            }
            match audio {
                // The file is the body; the words go beside it.
                Some((bytes, name)) => {
                    query.push(("message".into(), n.text.clone()));
                    query.push(("filename".into(), name.clone()));
                    Payload { content_type: audio_type(name).into(), body: bytes.clone(), query, multipart: false }
                }
                None => Payload { content_type: "text/plain; charset=utf-8".into(), body: n.text.clone().into_bytes(), query, multipart: false },
            }
        }
        Format::Json => {
            let mut v = serde_json::to_value(n).unwrap_or_default();
            if let Some((bytes, name)) = audio {
                v["audio"] = json!({ "name": name, "type": audio_type(name), "base64": trunk_recorder_plugin::base64::encode(bytes) });
            }
            json(v)
        }
    }
}

/// An event's colour (Discord's embeds).
fn color(event: &str) -> u32 {
    match event {
        "emergency" => 0xE53935,
        "signal" => 0xFB8C00,
        "tone" => 0x1E88E5,
        "recording" => 0x43A047,
        "new_talkgroup" | "new_radio" => 0x8E24AA,
        _ => 0x757575,
    }
}

/// An event's ntfy tag (an emoji).
fn tag(event: &str) -> &'static str {
    match event {
        "emergency" => "rotating_light",
        "signal" | "tone" => "bell",
        "recording" => "speaker",
        "new_talkgroup" | "new_radio" => "new",
        _ => "radio",
    }
}

/// What a notification says beside its words: (name, value) for each field it has.
fn fields(n: &Notification) -> Vec<(&'static str, String)> {
    let mut f = Vec::new();
    if !n.short_name.is_empty() {
        f.push(("System", if n.site.is_empty() { n.short_name.clone() } else { format!("{} / {}", n.short_name, n.site) }));
    }
    if let Some(tg) = n.talkgroup {
        f.push(("Talkgroup", if n.alpha_tag.is_empty() { tg.to_string() } else { format!("{tg} {}", n.alpha_tag) }));
    }
    if let Some(u) = n.unit {
        f.push(("Radio", if n.unit_tag.is_empty() { u.to_string() } else { format!("{u} ({})", n.unit_tag) }));
    }
    if !n.signals.is_empty() {
        f.push(("Signal", n.signals.join(", ")));
    }
    if let Some(t) = n.detail["text"].as_str().filter(|t| !t.is_empty() && n.event != "recording") {
        f.push(("Tones", t.to_string()));
    }
    f
}

/// Unix seconds as ISO 8601, UTC.
fn iso(t: f64) -> String {
    let secs = t.floor() as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn discord(n: &Notification) -> Value {
    let mut embed = json!({
        "title": n.title,
        "description": n.text,
        "color": color(&n.event),
        "timestamp": iso(n.time),
        "fields": fields(n).into_iter().map(|(k, v)| json!({ "name": k, "value": v, "inline": true })).collect::<Vec<_>>(),
        "footer": { "text": format!("Trunk Recorder Pro · {}", n.rule) },
    });
    if !n.url.is_empty() {
        embed["url"] = json!(n.url);
    }
    json!({ "username": "Trunk Recorder Pro", "embeds": [embed] })
}

fn slack(n: &Notification) -> Value {
    let context = fields(n).into_iter().map(|(k, v)| format!("*{k}:* {v}")).collect::<Vec<_>>().join("   ");
    let mut text = format!("*{}*\n{}", n.title, n.text);
    if !n.url.is_empty() {
        text.push_str(&format!("\n<{}|Play the recording>", n.url));
    }
    let mut blocks = vec![json!({ "type": "section", "text": { "type": "mrkdwn", "text": text } })];
    if !context.is_empty() {
        blocks.push(json!({ "type": "context", "elements": [{ "type": "mrkdwn", "text": context }] }));
    }
    json!({ "text": format!("{}: {}", n.title, n.text), "blocks": blocks })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates() {
        let mut n = test_notification();
        (n.short_name, n.talkgroup, n.signals, n.text) = ("CLMRN".into(), Some(401), vec!["CLMRN:CSP_FIRE".into()], "CSP fire page".into());
        assert_eq!(render("{signals} on TG {talkgroup}: {text}", &n), "CLMRN:CSP_FIRE on TG 401: CSP fire page");
        // A JSON template (Discord's plain one): values escaped.
        n.text = "Siren \"rise and fall\"".into();
        let discord: Value = serde_json::from_str(&render(r#"{"content": "{text}"}"#, &n)).unwrap();
        assert_eq!(discord["content"], "Siren \"rise and fall\"");
        let json: Value = serde_json::from_str(&render("", &n)).unwrap();
        assert_eq!((json["rule"].as_str(), json["talkgroup"].as_u64()), (Some("Test"), Some(401)));
    }

    #[test]
    fn formats() {
        let mut n = test_notification();
        (n.event, n.title, n.text, n.short_name, n.talkgroup, n.alpha_tag, n.signals, n.time) = (
            "signal".into(),
            "LOCALTOWN FIRE PAGE".into(),
            "wood:FD_PAGE".into(),
            "wood".into(),
            Some(1),
            "Fire Dispatch".into(),
            vec!["wood:FD_PAGE".into()],
            1792613980.0,
        );
        let d = |format: Format| Destination { name: "x".into(), format, ..Default::default() };
        let discord: Value = serde_json::from_slice(&payload(&d(Format::Discord), &n, None).body).unwrap();
        let e = &discord["embeds"][0];
        assert_eq!(
            (e["title"].as_str(), e["color"].as_u64(), e["timestamp"].as_str()),
            (Some("LOCALTOWN FIRE PAGE"), Some(0xFB8C00), Some("2026-10-21T20:19:40Z"))
        );
        assert_eq!(e["fields"][1], json!({ "name": "Talkgroup", "value": "1 Fire Dispatch", "inline": true }));
        let file: Audio = (vec![1, 2, 3], "a.m4a".into());
        assert!(!payload(&d(Format::Discord), &n, Some(&file)).multipart, "only a recording carries its file");
        n.event = "recording".into();
        assert!(payload(&d(Format::Discord), &n, Some(&file)).multipart);
        // ntfy: the file is the body, the words in the query; JSON: inline.
        let ntfy = payload(&d(Format::Ntfy), &n, Some(&file));
        assert_eq!((ntfy.body.clone(), ntfy.content_type.as_str()), (vec![1, 2, 3], "audio/mp4"));
        assert!(
            ntfy.query.contains(&("filename".to_string(), "a.m4a".to_string())) && ntfy.query.contains(&("message".to_string(), "wood:FD_PAGE".to_string()))
        );
        let inline: Value = serde_json::from_slice(&payload(&d(Format::Json), &n, Some(&file)).body).unwrap();
        assert_eq!((inline["audio"]["name"].as_str(), inline["audio"]["base64"].as_str()), (Some("a.m4a"), Some("AQID")));
        n.event = "signal".into();
        let slack: Value = serde_json::from_slice(&payload(&d(Format::Slack), &n, None).body).unwrap();
        assert_eq!(slack["text"], "LOCALTOWN FIRE PAGE: wood:FD_PAGE");
        let ntfy = payload(&d(Format::Ntfy), &n, None);
        assert_eq!((ntfy.body.as_slice(), ntfy.query[0].clone()), (&b"wood:FD_PAGE"[..], ("title".to_string(), "LOCALTOWN FIRE PAGE".to_string())));
        // Its own body wins over the format.
        let t = Destination { body: "{title} {url}".into(), ..d(Format::Discord) };
        n.url = "http://x/calls/a.m4a".into();
        assert_eq!(payload(&t, &n, None).body, b"LOCALTOWN FIRE PAGE http://x/calls/a.m4a");
        assert_eq!(iso(0.0), "1970-01-01T00:00:00Z");
    }
}
