//! Capability `energy`'s buffers (docs/mods.md, "Energy"): one per block
//! entity that has one, kept by the mod owning that block entity and shared
//! with every mod. They live beside the block entities
//! ([`crate::BlockEntities::energy`]), are lent and saved with them, in
//! their own file.
use crate::block_entities::Key;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The file, from the world folder.
pub const FILE: &str = "data/horizon/energy.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Storage {
    pub stored: u64,
    pub capacity: u64,
    /// The most an `insert` puts in, and an `extract` takes out, per call.
    pub max_receive: u64,
    pub max_extract: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Buffers {
    /// By position: the owning mod's id and its buffer.
    at: BTreeMap<Key, (String, Storage)>,
    pub(crate) dirty: bool,
}

/// One buffer as the file holds it.
#[derive(Deserialize, Serialize)]
struct Saved {
    owner: String,
    dimension: String,
    pos: [i32; 3],
    #[serde(flatten)]
    storage: Storage,
}

/// The six positions beside `key`.
fn sides(key: &Key) -> [Key; 6] {
    let (d, [x, y, z]) = key;
    [
        [x - 1, *y, *z],
        [x + 1, *y, *z],
        [*x, y - 1, *z],
        [*x, y + 1, *z],
        [*x, *y, z - 1],
        [*x, *y, z + 1],
    ]
    .map(|p| (d.clone(), p))
}

impl Buffers {
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
        let at = saved
            .into_iter()
            .map(|s| {
                let mut storage = s.storage;
                storage.stored = storage.stored.min(storage.capacity);
                ((s.dimension, s.pos), (s.owner, storage))
            })
            .collect();
        Self { at, dirty: false }
    }

    pub(crate) fn save(&mut self, world: &Path) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let saved: Vec<Saved> = self
            .at
            .iter()
            .map(|((dimension, pos), (owner, storage))| Saved {
                owner: owner.clone(),
                dimension: dimension.clone(),
                pos: *pos,
                storage: *storage,
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

    pub fn get(&self, key: &Key) -> Option<Storage> {
        self.at.get(key).map(|(_, s)| *s)
    }

    /// Gives `owner`'s block entity at `key` a buffer, or changes its limits.
    /// The caller checks that `owner` has a block entity there.
    pub fn set_storage(
        &mut self,
        owner: &str,
        key: Key,
        capacity: u64,
        max_receive: u64,
        max_extract: u64,
    ) -> Result<(), String> {
        let stored = match self.at.get(&key) {
            Some((o, _)) if o != owner => {
                return Err(format!(
                    "mod {o} keeps a buffer at {:?} in {}",
                    key.1, key.0
                ));
            }
            Some((_, s)) => s.stored.min(capacity),
            None => 0,
        };
        let storage = Storage {
            stored,
            capacity,
            max_receive,
            max_extract,
        };
        if self.get(&key) != Some(storage) {
            self.at.insert(key, (owner.to_owned(), storage));
            self.dirty = true;
        }
        Ok(())
    }

    /// `owner`'s own buffer at `key`, for it to change.
    fn own(&mut self, owner: &str, key: &Key) -> Result<&mut Storage, String> {
        match self.at.get_mut(key) {
            Some((o, s)) if o == owner => Ok(s),
            _ => Err(format!("no buffer of this mod at {:?} in {}", key.1, key.0)),
        }
    }

    pub fn set_stored(&mut self, owner: &str, key: &Key, stored: u64) -> Result<(), String> {
        let s = self.own(owner, key)?;
        let stored = stored.min(s.capacity);
        if s.stored != stored {
            s.stored = stored;
            self.dirty = true;
        }
        Ok(())
    }

    pub fn insert(&mut self, key: &Key, amount: u64, simulate: bool) -> u64 {
        let Some((_, s)) = self.at.get_mut(key) else {
            return 0;
        };
        let n = amount.min(s.max_receive).min(s.capacity - s.stored);
        if !simulate && n > 0 {
            s.stored += n;
            self.dirty = true;
        }
        n
    }

    pub fn extract(&mut self, key: &Key, amount: u64, simulate: bool) -> u64 {
        let Some((_, s)) = self.at.get_mut(key) else {
            return 0;
        };
        let n = amount.min(s.max_extract).min(s.stored);
        if !simulate && n > 0 {
            s.stored -= n;
            self.dirty = true;
        }
        n
    }

    /// Moves up to `amount` from `owner`'s buffer at `key` into the buffers
    /// beside it, shared evenly (the remainder to the first sides), each as
    /// far as it takes: what one leaves goes round again to those with room.
    /// What moved.
    pub fn push(&mut self, owner: &str, key: &Key, amount: u64) -> Result<u64, String> {
        let available = self.own(owner, key)?.stored.min(amount);
        let mut takers: Vec<Key> = sides(key)
            .into_iter()
            .filter(|k| self.insert(k, 1, true) > 0)
            .collect();
        let mut moved = 0;
        // Each round fills at least one side or moves everything: six at most.
        while !takers.is_empty() && moved < available {
            let left = available - moved;
            let n = takers.len() as u64;
            let mut full = Vec::new();
            for (i, k) in takers.iter().enumerate() {
                let share = left / n + u64::from((i as u64) < left % n);
                let took = self.insert(k, share, false);
                moved += took;
                if took < share {
                    full.push(i);
                }
            }
            if full.is_empty() {
                break;
            }
            for i in full.into_iter().rev() {
                takers.remove(i);
            }
        }
        self.own(owner, key)?.stored -= moved;
        Ok(moved)
    }

    /// Drops `owner`'s buffer at `key`, if it keeps one there.
    pub fn remove(&mut self, owner: &str, key: &Key) {
        if self.at.get(key).is_some_and(|(o, _)| o == owner) {
            self.at.remove(key);
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(x: i32) -> Key {
        ("minecraft:overworld".into(), [x, 64, 0])
    }

    #[test]
    fn insert_and_extract_keep_the_limits() {
        let mut b = Buffers::default();
        b.set_storage("m", key(0), 100, 30, 20).unwrap();
        assert_eq!(b.insert(&key(0), 50, true), 30);
        assert_eq!(b.get(&key(0)).unwrap().stored, 0);
        assert_eq!(b.insert(&key(0), 50, false), 30);
        assert_eq!(b.extract(&key(0), 50, false), 20);
        assert_eq!(b.get(&key(0)).unwrap().stored, 10);
        assert_eq!(b.insert(&key(9), 5, false), 0);
        b.set_stored("m", &key(0), 500).unwrap();
        assert_eq!(b.get(&key(0)).unwrap().stored, 100);
        assert!(b.set_stored("other", &key(0), 1).is_err());
        assert!(b.set_storage("other", key(0), 1, 1, 1).is_err());
        b.set_storage("m", key(0), 40, 30, 20).unwrap();
        assert_eq!(b.get(&key(0)).unwrap().stored, 40);
    }

    #[test]
    fn push_shares_with_the_sides_that_take() {
        let mut b = Buffers::default();
        b.set_storage("gen", key(0), 1000, 0, 0).unwrap();
        b.set_stored("gen", &key(0), 101).unwrap();
        b.set_storage("other", key(1), 1000, 1000, 0).unwrap();
        b.set_storage("other", key(-1), 10, 1000, 0).unwrap();
        b.set_storage("other", key(5), 1000, 1000, 0).unwrap();
        // -1 takes 10 of its 51; the 41 it leaves go round to +1.
        assert_eq!(b.push("gen", &key(0), 1000).unwrap(), 101);
        assert_eq!(b.get(&key(1)).unwrap().stored, 91);
        assert_eq!(b.get(&key(-1)).unwrap().stored, 10);
        assert_eq!(b.get(&key(5)).unwrap().stored, 0);
        assert_eq!(b.get(&key(0)).unwrap().stored, 0);
        b.set_stored("gen", &key(0), 7).unwrap();
        assert_eq!(b.push("gen", &key(0), 3).unwrap(), 3);
        assert_eq!(b.get(&key(0)).unwrap().stored, 4);
        assert!(b.push("other", &key(0), 1).is_err());
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("horizon-energy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut b = Buffers::default();
        b.set_storage("m", key(3), 100, 10, 10).unwrap();
        b.set_stored("m", &key(3), 42).unwrap();
        b.save(&dir).unwrap();
        let back = Buffers::load(&dir);
        assert_eq!(back.get(&key(3)).unwrap().stored, 42);
        let mut back = back;
        back.remove("m", &key(3));
        assert_eq!(back.get(&key(3)), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
