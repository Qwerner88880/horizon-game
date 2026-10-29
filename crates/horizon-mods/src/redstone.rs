//! Capability `redstone`'s state (docs/mods.md, "Redstone"): what a mod's
//! block entities give out as redstone signals (kept, saved with the world,
//! in their own file) and what the game last read around them (read fresh
//! each tick, never saved). They live beside the block entities
//! ([`crate::BlockEntities::redstone`]) and are lent with them.
use crate::block_entities::Key;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The file, from the world folder.
pub const FILE: &str = "data/horizon/redstone.json";

/// Signal strengths (0 to 15) on a block's six faces, in the order down, up,
/// north, south, west, east.
pub type Sides = [u8; 6];

/// What a block gives out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Output {
    /// The strength on each face: what the block touching that face reads.
    pub power: Sides,
    /// Also powers the blocks it touches through (as a lever does the block
    /// it is on), not only what reads it directly (as a redstone block does).
    pub strong: bool,
}

impl Output {
    /// Whether it gives nothing.
    pub fn is_off(&self) -> bool {
        self.power == [0; 6]
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Redstone {
    /// By position: the owning mod's id and what its block gives out.
    outputs: BTreeMap<Key, (String, Output)>,
    /// What each block of a redstone mod reads from the faces around it,
    /// set by the game before the mod is called.
    pub inputs: BTreeMap<Key, Sides>,
    /// What the mod was last told for each block.
    pub(crate) told: BTreeMap<Key, Sides>,
    pub(crate) dirty: bool,
}

/// One block's output as the file holds it.
#[derive(Deserialize, Serialize)]
struct Saved {
    owner: String,
    dimension: String,
    pos: [i32; 3],
    #[serde(flatten)]
    output: Output,
}

impl Redstone {
    pub(crate) fn load(world: &Path) -> Self {
        let path = world.join(FILE);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        let Ok(saved) = serde_json::from_str::<Vec<Saved>>(&text) else {
            log::warn!("Horizon: {} does not parse: kept as .bad", path.display());
            let _ = std::fs::rename(&path, path.with_extension("json.bad"));
            return Self::default();
        };
        let outputs = saved
            .into_iter()
            .map(|mut s| {
                s.output.power = s.output.power.map(|p| p.min(15));
                ((s.dimension, s.pos), (s.owner, s.output))
            })
            .filter(|(_, (_, o))| !o.is_off())
            .collect();
        Self {
            outputs,
            ..Self::default()
        }
    }

    pub(crate) fn save(&mut self, world: &Path) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let saved: Vec<Saved> = self
            .outputs
            .iter()
            .map(|((dimension, pos), (owner, output))| Saved {
                owner: owner.clone(),
                dimension: dimension.clone(),
                pos: *pos,
                output: *output,
            })
            .collect();
        let path = world.join(FILE);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(&saved)?)?;
        std::fs::rename(&tmp, &path)?;
        self.dirty = false;
        Ok(())
    }

    /// What the block at `key` gives out, whichever mod drives it.
    pub fn output(&self, key: &Key) -> Option<Output> {
        self.outputs.get(key).map(|(_, o)| *o)
    }

    /// Every block that gives out a signal: its place and what it gives.
    pub fn outputs(&self) -> impl Iterator<Item = (&Key, &Output)> {
        self.outputs.iter().map(|(k, (_, o))| (k, o))
    }

    /// Sets what `owner`'s block entity at `key` gives out (strengths over 15
    /// are an error; nothing at all removes it). Whether it changed. The
    /// caller checks that `owner` has a block entity there.
    pub fn set_output(&mut self, owner: &str, key: Key, output: Output) -> Result<bool, String> {
        if output.power.iter().any(|&p| p > 15) {
            return Err("a redstone signal is 0 to 15".into());
        }
        if let Some((o, _)) = self.outputs.get(&key)
            && o != owner
        {
            return Err(format!(
                "mod {o} drives the signal at {:?} in {}",
                key.1, key.0
            ));
        }
        let output = Output {
            // A strong signal of nothing is nothing.
            strong: output.strong && !output.is_off(),
            ..output
        };
        let before = self.output(&key);
        if output.is_off() {
            if before.is_none() {
                return Ok(false);
            }
            self.outputs.remove(&key);
        } else if before == Some(output) {
            return Ok(false);
        } else {
            self.outputs.insert(key, (owner.to_owned(), output));
        }
        self.dirty = true;
        Ok(true)
    }

    /// Drops what `owner`'s block entity at `key` gave out and read: whether
    /// it was giving a signal.
    pub fn remove(&mut self, owner: &str, key: &Key) -> bool {
        self.inputs.remove(key);
        self.told.remove(key);
        if self.outputs.get(key).is_some_and(|(o, _)| o == owner) {
            self.outputs.remove(key);
            self.dirty = true;
            return true;
        }
        false
    }

    /// The blocks whose reading changed since the mod was last told of it
    /// (or that it has never been told of), with what they read now; marks
    /// them told. `owned` is the mod's own block entities.
    pub fn changed_inputs<'a>(
        &mut self,
        owned: impl Iterator<Item = &'a Key>,
    ) -> Vec<(Key, Sides)> {
        let mut out = Vec::new();
        for key in owned {
            let Some(now) = self.inputs.get(key).copied() else {
                continue;
            };
            if self.told.get(key) != Some(&now) {
                self.told.insert(key.clone(), now);
                out.push((key.clone(), now));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(x: i32) -> Key {
        ("minecraft:overworld".into(), [x, 64, 0])
    }
    fn out(p: u8, strong: bool) -> Output {
        Output {
            power: [p; 6],
            strong,
        }
    }

    #[test]
    fn an_output_is_set_changed_and_cleared() {
        let mut r = Redstone::default();
        assert_eq!(r.set_output("m", key(0), out(15, false)), Ok(true));
        assert_eq!(r.set_output("m", key(0), out(15, false)), Ok(false));
        assert_eq!(r.output(&key(0)), Some(out(15, false)));
        assert_eq!(r.set_output("m", key(0), out(7, true)), Ok(true));
        // Off removes it; a strong nothing is nothing.
        assert_eq!(r.set_output("m", key(0), out(0, true)), Ok(true));
        assert_eq!(r.output(&key(0)), None);
        assert_eq!(r.set_output("m", key(0), out(0, true)), Ok(false));
        assert!(r.set_output("m", key(0), out(16, false)).is_err());
    }

    #[test]
    fn only_the_driver_changes_an_output() {
        let mut r = Redstone::default();
        r.set_output("m", key(0), out(3, false)).unwrap();
        assert!(r.set_output("other", key(0), out(1, false)).is_err());
        assert!(!r.remove("other", &key(0)));
        assert!(r.remove("m", &key(0)));
        assert_eq!(r.output(&key(0)), None);
        assert!(!r.remove("m", &key(0)));
    }

    #[test]
    fn a_mod_is_told_of_a_reading_once_and_when_it_changes() {
        let mut r = Redstone::default();
        let owned = [key(0), key(1)];
        // Nothing read yet: nothing to tell.
        assert!(r.changed_inputs(owned.iter()).is_empty());
        r.inputs.insert(key(0), [0, 0, 0, 0, 0, 15]);
        let told = r.changed_inputs(owned.iter());
        assert_eq!(told, vec![(key(0), [0, 0, 0, 0, 0, 15])]);
        assert!(r.changed_inputs(owned.iter()).is_empty());
        r.inputs.insert(key(0), [0; 6]);
        r.inputs.insert(key(1), [1; 6]);
        assert_eq!(r.changed_inputs(owned.iter()).len(), 2);
        r.remove("m", &key(1));
        assert!(r.changed_inputs(owned.iter()).is_empty());
    }

    #[test]
    fn saves_and_loads_the_outputs_only() {
        let dir = std::env::temp_dir().join(format!("horizon-redstone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut r = Redstone::default();
        r.set_output("m", key(3), out(9, true)).unwrap();
        r.inputs.insert(key(3), [1; 6]);
        r.save(&dir).unwrap();
        let back = Redstone::load(&dir);
        assert_eq!(back.output(&key(3)), Some(out(9, true)));
        assert!(back.inputs.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
