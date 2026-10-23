//! Sending: one thread takes each notification to its destinations, so a
//! slow webhook never holds up the recorder's messages. Webhooks have a
//! timeout and are tried twice more; commands get 30 seconds. How each
//! destination is doing goes to the recorder's dashboard (metrics).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use trunk_recorder_plugin::{Endpoint, EndpointState, Host, Metrics};

use crate::payload::{self, audio_type, command_env, payload, Notification};
use crate::settings::{Destination, Kind};

const TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
/// A webhook that failed is tried again after these.
const RETRIES: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(10)];

/// A recording's audio, and its file name, shared by its destinations.
pub type Audio = Arc<payload::Audio>;

struct Job {
    n: Notification,
    dest: Destination,
    audio: Option<Audio>,
}

/// How a destination has been doing.
#[derive(Default)]
struct Health {
    sent: u64,
    failed: u64,
    last_ok: Option<f64>,
    last_error: String,
    latency_ms: Option<f64>,
}

/// The sending thread.
pub struct Sender {
    tx: mpsc::Sender<Job>,
}

impl Sender {
    pub fn start(host: Host) -> Sender {
        let (tx, rx) = mpsc::channel::<Job>();
        let health: Arc<Mutex<BTreeMap<String, Health>>> = Default::default();
        std::thread::Builder::new()
            .name("notify".into())
            .spawn(move || {
                for job in rx {
                    let started = Instant::now();
                    let result = send(&job.dest, &job.n, job.audio.as_deref());
                    let mut all = health.lock().unwrap();
                    let h = all.entry(job.dest.name.clone()).or_default();
                    match &result {
                        Ok(()) => {
                            h.sent += 1;
                            h.last_ok = Some(crate::now());
                            h.last_error.clear();
                            h.latency_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
                            host.info(format!("{} → {}: sent", job.n.title, job.dest.name));
                        }
                        Err(e) => {
                            h.failed += 1;
                            h.last_error = e.clone();
                            host.warn(format!("{} → {}: {e}", job.n.title, job.dest.name));
                        }
                    }
                    host.metrics(&metrics(&all));
                }
            })
            .expect("a thread to send notifications from");
        Sender { tx }
    }

    pub fn send(&self, n: Notification, dest: Destination, audio: Option<Audio>) {
        let _ = self.tx.send(Job { n, dest, audio });
    }
}

fn metrics(all: &BTreeMap<String, Health>) -> Metrics {
    Metrics {
        endpoints: all
            .iter()
            .map(|(name, h)| Endpoint {
                name: name.clone(),
                state: if !h.last_error.is_empty() { EndpointState::Down } else { EndpointState::Up },
                latency_ms: h.latency_ms,
                last_ok: h.last_ok,
                last_error: h.last_error.clone(),
            })
            .collect(),
        extra: [
            ("sent".to_string(), all.values().map(|h| h.sent).sum::<u64>().into()),
            ("failed".to_string(), all.values().map(|h| h.failed).sum::<u64>().into()),
        ]
        .into(),
        ..Default::default()
    }
}

/// `n` to `dest`, now, on this thread: how it went.
pub fn send(dest: &Destination, n: &Notification, audio: Option<&(Vec<u8>, String)>) -> Result<(), String> {
    match dest.kind {
        Kind::Webhook => webhook(dest, n, audio),
        Kind::Command => command(dest, n, audio),
    }
}

/// A multipart body: Discord's `payload_json` and the file.
fn multipart(json: &[u8], file: &(Vec<u8>, String)) -> (String, Vec<u8>) {
    let boundary = format!("trp-{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    let kind = audio_type(&file.1);
    let mut b = Vec::with_capacity(json.len() + file.0.len() + 512);
    b.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"payload_json\"\r\nContent-Type: application/json\r\n\r\n").as_bytes());
    b.extend_from_slice(json);
    b.extend_from_slice(
        format!("\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"files[0]\"; filename=\"{}\"\r\nContent-Type: {kind}\r\n\r\n", file.1).as_bytes(),
    );
    b.extend_from_slice(&file.0);
    b.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), b)
}

fn webhook(dest: &Destination, n: &Notification, audio: Option<&(Vec<u8>, String)>) -> Result<(), String> {
    if !(dest.url.starts_with("http://") || dest.url.starts_with("https://")) {
        return Err("its URL must begin http:// or https://".into());
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .user_agent(concat!("trunk-plugin-notify/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let p = payload(dest, n, audio);
    let (content_type, body) = match audio.filter(|_| p.multipart) {
        Some(file) => multipart(&p.body, file),
        None => (p.content_type.clone(), p.body.clone()),
    };
    let url = with_query(&dest.url, &p.query);
    let method = if dest.method.trim().is_empty() { "POST".to_string() } else { dest.method.trim().to_ascii_uppercase() };
    let own = dest.header_list();
    let mut last = String::new();
    for wait in std::iter::once(Duration::ZERO).chain(RETRIES) {
        std::thread::sleep(wait);
        let mut req = ureq::http::Request::builder().method(method.as_str()).uri(&url);
        if !own.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
            req = req.header("Content-Type", content_type.as_str());
        }
        for (k, v) in &own {
            req = req.header(k.as_str(), v.as_str());
        }
        let req = req.body(body.clone()).map_err(|e| format!("couldn't make the request: {e}"))?;
        match agent.run(req) {
            Ok(r) if r.status().is_success() => return Ok(()),
            // (A client error won't go better for trying again.)
            Ok(r) if r.status().is_client_error() => return Err(format!("it answered {}", r.status())),
            Ok(r) => last = format!("it answered {}", r.status()),
            Err(e) => last = format!("couldn't reach it: {e}"),
        }
    }
    Err(last)
}

/// `url` with `query` added, each value percent-encoded.
fn with_query(url: &str, query: &[(String, String)]) -> String {
    let enc = |s: &str| {
        s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>()
    };
    query.iter().fold(url.to_string(), |u, (k, v)| format!("{u}{}{k}={}", if u.contains('?') { '&' } else { '?' }, enc(v)))
}

/// A recording for a command: written to a file of its own (`TRP_AUDIO`), removed when it's done.
struct AudioFile(std::path::PathBuf);

impl Drop for AudioFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn command(dest: &Destination, n: &Notification, audio: Option<&payload::Audio>) -> Result<(), String> {
    if dest.command.trim().is_empty() {
        return Err("no command".into());
    }
    let file = match audio.filter(|_| n.event == "recording") {
        Some((bytes, name)) => {
            let path = std::env::temp_dir().join(format!("trp-notify-{}-{name}", n.id));
            std::fs::write(&path, bytes).map_err(|e| format!("couldn't write the recording for it: {e}"))?;
            Some(AudioFile(path))
        }
        None => None,
    };
    let mut c = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(&dest.command);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c").arg(&dest.command);
        c
    };
    // (Its output isn't ours: the recorder reads this plugin's stdout.)
    c.envs(command_env(n)).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
    if let Some(f) = &file {
        c.env("TRP_AUDIO", &f.0);
    }
    let mut child = c.spawn().map_err(|e| format!("couldn't run it: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(serde_json::to_string(n).unwrap_or_default().as_bytes());
    }
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
                let err = err.lines().last().unwrap_or("").trim().to_string();
                return Err(format!("it ended with {status}{}", if err.is_empty() { String::new() } else { format!(": {err}") }));
            }
            Ok(None) if started.elapsed() > COMMAND_TIMEOUT => {
                let _ = child.kill();
                return Err(format!("it ran past {} s and was stopped", COMMAND_TIMEOUT.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// 16-bit mono WAV of `samples` (-1..1) at 8 kHz.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&8000u32.to_le_bytes());
    b.extend_from_slice(&16000u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    b
}

/// A destination tried now, as edited: a test notification, or (`recording`)
/// one as a call's recording follows, with a few seconds of a two-tone page.
pub fn test(dest: &Destination, recording: bool) -> Result<(), String> {
    let mut n = crate::payload::test_notification();
    if !recording {
        return send(dest, &n, None);
    }
    n.event = "recording".into();
    n.text = format!("{} (recording, 4 s)", n.text);
    let page: Vec<f32> = (0..32_000).map(|i| 0.3 * (2.0 * std::f32::consts::PI * if i < 8000 { 467.2 } else { 1032.3 } * i as f32 / 8000.0).sin()).collect();
    send(dest, &n, Some(&(wav(&page), "test-page.wav".into())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn commands_get_the_notification() {
        let dir = std::env::temp_dir().join(format!("notify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("out.txt");
        let dest = Destination {
            name: "x".into(),
            kind: Kind::Command,
            command: format!("printf '%s|%s|' \"$TRP_RULE\" \"$TRP_TEXT\" > {0}; cat >> {0}", out.display()),
            ..Default::default()
        };
        let mut n = crate::payload::test_notification();
        (n.rule, n.text) = ("csp".into(), "CLMRN:CSP_FIRE on TG 401".into());
        send(&dest, &n, None).unwrap();
        let got = std::fs::read_to_string(&out).unwrap();
        assert!(got.starts_with("csp|CLMRN:CSP_FIRE on TG 401|{"), "{got}");
        let fail = Destination { command: "echo nope >&2; exit 3".into(), ..dest.clone() };
        let e = send(&fail, &n, None).unwrap_err();
        assert!(e.contains("nope"), "{e}");
        // A recording: its file, for as long as the command runs.
        let size = dir.join("size.txt");
        let rec = Destination { command: format!("wc -c < \"$TRP_AUDIO\" > {}", size.display()), ..dest };
        n.event = "recording".into();
        send(&rec, &n, Some(&(vec![1, 2, 3], "call.wav".into()))).unwrap();
        assert_eq!(std::fs::read_to_string(&size).unwrap().trim(), "3");
        assert!(!std::env::temp_dir().join(format!("trp-notify-{}-call.wav", n.id)).exists(), "removed after");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discord_gets_the_file() {
        let (ct, body) = multipart(br#"{"embeds":[]}"#, &(vec![1, 2, 3], "20341-1792.m4a".into()));
        let boundary = ct.strip_prefix("multipart/form-data; boundary=").unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.starts_with(&format!("--{boundary}\r\nContent-Disposition: form-data; name=\"payload_json\"")), "{text}");
        assert!(text.contains("name=\"files[0]\"; filename=\"20341-1792.m4a\"\r\nContent-Type: audio/mp4"), "{text}");
        assert!(text.ends_with(&format!("--{boundary}--\r\n")));
    }

    #[test]
    fn query_values_are_encoded() {
        let q = [("title".to_string(), "FIRE → Station 4".to_string()), ("filename".to_string(), "a.m4a".to_string())];
        assert_eq!(with_query("https://ntfy.sh/t", &q), "https://ntfy.sh/t?title=FIRE%20%E2%86%92%20Station%204&filename=a.m4a");
    }

    #[test]
    fn webhooks_say_why_they_failed() {
        let bad = Destination { name: "x".into(), url: "ftp://example".into(), ..Default::default() };
        assert!(send(&bad, &crate::payload::test_notification(), None).unwrap_err().contains("http"));
        assert_eq!(&wav(&[0.0; 4])[..4], b"RIFF");
    }
}
