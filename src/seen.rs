//! The talkgroups and radios each system has been heard with, kept in the
//! data folder, so "a new talkgroup" means one never heard before. A system
//! is learned for its first hour here: starting with a new system doesn't
//! announce everything on it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A system is learned this long before anything on it counts as new, s.
const WARM_UP_S: f64 = 3600.0;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct System {
    /// When it was first heard here, Unix s.
    since: f64,
    talkgroups: BTreeSet<u32>,
    units: BTreeSet<u32>,
}

#[derive(Default)]
pub struct Seen {
    path: PathBuf,
    systems: BTreeMap<String, System>,
    changed: bool,
}

/// What a call brought that wasn't heard before.
#[derive(Debug, Default, PartialEq)]
pub struct Firsts {
    pub talkgroup: bool,
    pub units: Vec<u32>,
}

impl Seen {
    /// What `data_dir` has kept (nothing, the first time).
    pub fn load(data_dir: &Path) -> Seen {
        let path = data_dir.join("seen.json");
        let systems = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Seen { path, systems, changed: false }
    }

    /// A call on `system`'s `talkgroup` with `units` at `now`: what's new (none while the system is warming up).
    pub fn heard(&mut self, system: &str, talkgroup: u32, units: &[u32], now: f64) -> Firsts {
        let s = self.systems.entry(system.to_string()).or_insert_with(|| System { since: now, ..Default::default() });
        let warm = now - s.since >= WARM_UP_S;
        let mut f = Firsts { talkgroup: s.talkgroups.insert(talkgroup), units: Vec::new() };
        for &u in units.iter().filter(|&&u| u != 0) {
            if s.units.insert(u) {
                f.units.push(u);
            }
        }
        self.changed |= f.talkgroup || !f.units.is_empty();
        if !warm {
            f = Firsts::default();
        }
        f
    }

    /// Written to the data folder, if anything changed.
    pub fn save(&mut self) -> std::io::Result<()> {
        if !self.changed {
            return Ok(());
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(&self.systems)?)?;
        std::fs::rename(&tmp, &self.path)?;
        self.changed = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firsts_after_the_warm_up() {
        let dir = std::env::temp_dir().join(format!("notify-seen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Seen::load(&dir);
        assert_eq!(s.heard("wood", 1, &[100, 0], 1000.0), Firsts::default(), "learning, the first hour");
        assert_eq!(s.heard("wood", 2, &[101], 1000.0 + WARM_UP_S), Firsts { talkgroup: true, units: vec![101] });
        assert_eq!(s.heard("wood", 1, &[100], 1000.0 + WARM_UP_S), Firsts::default(), "heard while learning");
        s.save().unwrap();
        let mut again = Seen::load(&dir);
        assert_eq!(again.heard("wood", 2, &[101, 102], 1000.0 + WARM_UP_S), Firsts { talkgroup: false, units: vec![102] }, "kept between runs");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
