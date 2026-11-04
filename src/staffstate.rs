//! What staff tools keep across a restart and a rejoin, in `state.tsv`: who is vanished, who is
//! frozen, and where each watching staff member started (so a crash or a quit never leaves him
//! in spectator mode far from home).
//!
//! Lines (tab-separated, text fields escaped):
//! - `vanish  <uuid>`
//! - `frozen  <uuid>  <name>  <1 if he was invulnerable before, else 0>`
//! - `watch  <staff uuid>  <target uuid>  <game mode>  <world>  <x>  <y>  <z>  <yaw>  <pitch>`
//! - `exempt  <uuid>`: a player who held `admin-lite:exempt` when he was last online, so staff
//!   cannot punish him while he is offline either.

use std::collections::{BTreeMap, BTreeSet};

use crate::text;

#[derive(Clone, Debug, PartialEq)]
pub struct Frozen {
    pub name: String,
    pub was_invulnerable: bool,
}

/// The game mode a watching staff member goes back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl Mode {
    const ALL: [(Self, &'static str); 4] = [
        (Self::Survival, "survival"),
        (Self::Creative, "creative"),
        (Self::Adventure, "adventure"),
        (Self::Spectator, "spectator"),
    ];

    #[must_use]
    pub fn code(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(mode, _)| *mode == self)
            .map_or("survival", |(_, code)| code)
    }

    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, c)| *c == code)
            .map(|(mode, _)| *mode)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Watch {
    pub target_id: String,
    pub mode: Mode,
    pub world: String,
    pub pos: (f64, f64, f64),
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StaffState {
    pub vanished: BTreeSet<String>,
    pub frozen: BTreeMap<String, Frozen>,
    /// By the watching staff member's UUID.
    pub watching: BTreeMap<String, Watch>,
    pub exempt: BTreeSet<String>,
}

fn flag(text: &str) -> Option<bool> {
    match text {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

fn number<T: std::str::FromStr + Copy>(text: &str, finite: fn(T) -> bool) -> Option<T> {
    text.parse::<T>().ok().filter(|v| finite(*v))
}

impl StaffState {
    /// Reads the file's text; returns the state and how many lines did not read.
    #[must_use]
    pub fn from_text(text: &str) -> (Self, usize) {
        let mut state = Self::default();
        let mut bad = 0;
        for line in text
            .lines()
            .map(|l| l.trim_end_matches('\r'))
            .filter(|l| !l.is_empty())
        {
            if state.read_line(line).is_none() {
                bad += 1;
            }
        }
        (state, bad)
    }

    fn read_line(&mut self, line: &str) -> Option<()> {
        let f: Vec<&str> = line.split('\t').collect();
        let id = |i: usize| {
            f.get(i)
                .map(|s| text::unescape(s))
                .filter(|s| !s.is_empty())
        };
        match (f.first().copied(), f.len()) {
            (Some("vanish"), 2) => {
                self.vanished.insert(id(1)?);
            }
            (Some("frozen"), 4) => {
                let name = text::unescape(f[2]);
                if !text::valid_name(&name) {
                    return None;
                }
                let was_invulnerable = flag(f[3])?;
                self.frozen.insert(
                    id(1)?,
                    Frozen {
                        name,
                        was_invulnerable,
                    },
                );
            }
            (Some("watch"), 10) => {
                let watch = Watch {
                    target_id: id(2)?,
                    mode: Mode::from_code(f[3])?,
                    world: id(4)?,
                    pos: (
                        number(f[5], f64::is_finite)?,
                        number(f[6], f64::is_finite)?,
                        number(f[7], f64::is_finite)?,
                    ),
                    yaw: number(f[8], f32::is_finite)?,
                    pitch: number(f[9], f32::is_finite)?,
                };
                self.watching.insert(id(1)?, watch);
            }
            (Some("exempt"), 2) => {
                self.exempt.insert(id(1)?);
            }
            _ => return None,
        }
        Some(())
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        let bit = |b: bool| if b { "1" } else { "0" };
        let mut out = String::new();
        for id in &self.vanished {
            out.push_str(&format!("vanish\t{}\n", text::escape(id)));
        }
        for (id, f) in &self.frozen {
            out.push_str(&format!(
                "frozen\t{}\t{}\t{}\n",
                text::escape(id),
                text::escape(&f.name),
                bit(f.was_invulnerable)
            ));
        }
        for (id, w) in &self.watching {
            let (x, y, z) = w.pos;
            out.push_str(&format!(
                "watch\t{}\t{}\t{}\t{}\t{x}\t{y}\t{z}\t{}\t{}\n",
                text::escape(id),
                text::escape(&w.target_id),
                w.mode.code(),
                text::escape(&w.world),
                w.yaw,
                w.pitch
            ));
        }
        for id in &self.exempt {
            out.push_str(&format!("exempt\t{}\n", text::escape(id)));
        }
        out
    }

    /// The staff members watching `target_id`.
    #[must_use]
    pub fn watchers_of(&self, target_id: &str) -> Vec<String> {
        self.watching
            .iter()
            .filter(|(_, w)| w.target_id == target_id)
            .map(|(staff, _)| staff.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> StaffState {
        let mut state = StaffState::default();
        state.vanished.insert("u1".into());
        state.vanished.insert("u2".into());
        state.frozen.insert(
            "u3".into(),
            Frozen {
                name: "Alex".into(),
                was_invulnerable: false,
            },
        );
        state.watching.insert(
            "u1".into(),
            Watch {
                target_id: "u3".into(),
                mode: Mode::Creative,
                world: "my world\t2".into(),
                pos: (10.5, -64.0, -3.25),
                yaw: 90.0,
                pitch: -12.5,
            },
        );
        state.exempt.insert("u7".into());
        state
    }

    #[test]
    fn the_state_survives_the_file() {
        let state = sample();
        let text = state.to_text();
        assert_eq!(text.lines().count(), 5);
        let (read, bad) = StaffState::from_text(&text);
        assert_eq!(bad, 0);
        assert_eq!(read, state);
        assert_eq!(read.watchers_of("u3"), vec!["u1".to_string()]);
        assert!(read.watchers_of("u1").is_empty());
    }

    #[test]
    fn bad_lines_are_skipped() {
        let text = format!(
            "{}nonsense\nvanish\nfrozen\tu9\tBob\tmaybe\nwatch\tu5\tu6\tflying\tw\t0\t0\t0\t0\t0\nwatch\tu5\tu6\tsurvival\tw\tNaN\t0\t0\t0\t0\nwatch\tu5\tu6\tsurvival\tw\tinf\t0\t0\t0\t0\nexempt\n",
            sample().to_text()
        );
        let (read, bad) = StaffState::from_text(&text);
        assert_eq!(bad, 7);
        assert_eq!(read, sample());
    }

    #[test]
    fn game_modes_have_codes() {
        for (mode, code) in Mode::ALL {
            assert_eq!(mode.code(), code);
            assert_eq!(Mode::from_code(code), Some(mode));
        }
        assert_eq!(Mode::from_code("hardcore"), None);
    }
}
