//! Capability `block-entities`' store (docs/mods.md, "Block entities"): the
//! bytes each server mod keeps on placed copies of the blocks it claims,
//! by mod id and position. The game owns it, lends it to the mods for their
//! calls, and saves it in the world folder beside `level.dat`.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Bytes one block entity may hold.
pub const MAX_DATA: usize = 16 * 1024;
/// Block entities one mod may have, across every dimension.
pub const MAX_PER_MOD: usize = 1 << 16;
/// The file, from the world folder.
pub const FILE: &str = "data/horizon/block_entities.json";

/// A block entity's place: dimension and block position.
pub type Key = (String, [i32; 3]);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockEntities {
    mods: BTreeMap<String, BTreeMap<Key, Vec<u8>>>,
    /// Capability `energy`'s buffers on these block entities.
    pub energy: crate::energy::Buffers,
    /// Capability `fluids`' tanks on them.
    pub fluids: crate::fluids::Tanks,
    /// Capability `redstone`'s signals on them.
    pub redstone: crate::redstone::Redstone,
    /// Capability `items`' inventories on them.
    pub items: crate::items::Inventories,
    /// Capability `multiblocks`' watches on them.
    pub multiblocks: crate::multiblocks::Watches,
    /// Capability `saved-data`' keys on the world itself.
    pub saved_data: crate::saved_data::SavedData,
    /// Changed since the last [`BlockEntities::save`].
    dirty: bool,
}

/// One block entity as the file holds it.
#[derive(Deserialize, Serialize)]
struct Saved {
    dimension: String,
    pos: [i32; 3],
    /// The data, as lowercase hex.
    data: String,
}

/// The block's id in a state string: `minecraft:oak_log` of
/// `minecraft:oak_log[axis=y]`.
pub fn block_name(state: &str) -> &str {
    state.split_once('[').map_or(state, |(name, _)| name)
}

impl BlockEntities {
    pub fn path(world: &Path) -> PathBuf {
        world.join(FILE)
    }

    /// The world's saved block entities; none when it has no file yet. A
    /// file that doesn't parse is logged and kept aside, not lost.
    pub fn load(world: &Path) -> Self {
        let path = Self::path(world);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self {
                energy: crate::energy::Buffers::load(world),
                fluids: crate::fluids::Tanks::load(world),
                redstone: crate::redstone::Redstone::load(world),
                items: crate::items::Inventories::load(world),
                multiblocks: crate::multiblocks::Watches::load(world),
                saved_data: crate::saved_data::SavedData::load(world),
                ..Self::default()
            };
        };
        let parsed: Result<BTreeMap<String, Vec<Saved>>, _> = serde_json::from_str(&text);
        let Ok(saved) = parsed else {
            log::warn!("Horizon: {} does not parse: kept as .bad", path.display());
            let _ = std::fs::rename(&path, path.with_extension("json.bad"));
            // The mod data on the world itself is not this file's to lose.
            return Self {
                saved_data: crate::saved_data::SavedData::load(world),
                ..Self::default()
            };
        };
        let mut out = Self {
            energy: crate::energy::Buffers::load(world),
            fluids: crate::fluids::Tanks::load(world),
            redstone: crate::redstone::Redstone::load(world),
            items: crate::items::Inventories::load(world),
            multiblocks: crate::multiblocks::Watches::load(world),
            saved_data: crate::saved_data::SavedData::load(world),
            ..Self::default()
        };
        for (id, entities) in saved {
            let map = out.mods.entry(id).or_default();
            for e in entities {
                if let Some(data) = from_hex(&e.data).filter(|d| d.len() <= MAX_DATA) {
                    map.insert((e.dimension, e.pos), data);
                }
            }
        }
        out
    }

    /// Writes the file when anything changed since the last save, through a
    /// temporary file so a crash mid-write keeps the old one.
    pub fn save(&mut self, world: &Path) -> std::io::Result<()> {
        self.energy.save(world)?;
        self.fluids.save(world)?;
        self.redstone.save(world)?;
        self.items.save(world)?;
        self.multiblocks.save(world)?;
        self.saved_data.save(world)?;
        if !self.dirty {
            return Ok(());
        }
        let saved: BTreeMap<&str, Vec<Saved>> = self
            .mods
            .iter()
            .filter(|(_, m)| !m.is_empty())
            .map(|(id, m)| {
                let entities = m
                    .iter()
                    .map(|((dimension, pos), data)| Saved {
                        dimension: dimension.clone(),
                        pos: *pos,
                        data: to_hex(data),
                    })
                    .collect();
                (id.as_str(), entities)
            })
            .collect();
        let path = Self::path(world);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(&saved)?)?;
        std::fs::rename(&tmp, &path)?;
        self.dirty = false;
        Ok(())
    }

    /// A new, empty block entity of `mod_id` at `key`, unless it has one
    /// there already or is at [`MAX_PER_MOD`]. True when one was made.
    pub fn create(&mut self, mod_id: &str, key: Key) -> bool {
        let map = self.mods.entry(mod_id.to_owned()).or_default();
        if map.contains_key(&key) || map.len() >= MAX_PER_MOD {
            return false;
        }
        map.insert(key, Vec::new());
        self.dirty = true;
        true
    }

    pub fn get(&self, mod_id: &str, key: &Key) -> Option<&[u8]> {
        self.mods.get(mod_id)?.get(key).map(Vec::as_slice)
    }

    /// Replaces the data of an existing block entity.
    pub fn set(&mut self, mod_id: &str, key: &Key, data: Vec<u8>) -> Result<(), String> {
        if data.len() > MAX_DATA {
            return Err(format!("{} bytes: at most {MAX_DATA}", data.len()));
        }
        let slot = self
            .mods
            .get_mut(mod_id)
            .and_then(|m| m.get_mut(key))
            .ok_or_else(|| format!("no block entity of this mod at {:?} in {}", key.1, key.0))?;
        if *slot != data {
            *slot = data;
            self.dirty = true;
        }
        Ok(())
    }

    /// Removes a block entity, giving back its data.
    pub fn remove(&mut self, mod_id: &str, key: &Key) -> Option<Vec<u8>> {
        let data = self.mods.get_mut(mod_id)?.remove(key)?;
        self.dirty = true;
        Some(data)
    }

    /// Every block entity of `mod_id`, in position order.
    pub fn of(&self, mod_id: &str) -> impl Iterator<Item = (&Key, &[u8])> {
        self.mods
            .get(mod_id)
            .into_iter()
            .flatten()
            .map(|(k, d)| (k, d.as_slice()))
    }
}

pub(crate) fn to_hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(x: i32) -> Key {
        ("minecraft:overworld".into(), [x, 64, 0])
    }

    #[test]
    fn create_set_remove() {
        let mut be = BlockEntities::default();
        assert!(be.set("m", &key(0), vec![1]).is_err());
        assert!(be.create("m", key(0)));
        assert!(!be.create("m", key(0)));
        assert_eq!(be.get("m", &key(0)), Some(&[][..]));
        assert_eq!(be.get("other", &key(0)), None);
        be.set("m", &key(0), vec![1, 2]).unwrap();
        assert!(be.set("m", &key(0), vec![0; MAX_DATA + 1]).is_err());
        assert_eq!(be.remove("m", &key(0)), Some(vec![1, 2]));
        assert_eq!(be.of("m").count(), 0);
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("horizon-be-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut be = BlockEntities::default();
        be.create("m", key(1));
        be.set("m", &key(1), vec![0, 255, 16]).unwrap();
        be.create("n", key(-5));
        be.save(&dir).unwrap();
        let back = BlockEntities::load(&dir);
        assert_eq!(back.get("m", &key(1)), Some(&[0, 255, 16][..]));
        assert_eq!(back.get("n", &key(-5)), Some(&[][..]));
        std::fs::write(BlockEntities::path(&dir), "nope").unwrap();
        assert_eq!(BlockEntities::load(&dir).of("m").count(), 0);
        assert!(
            BlockEntities::path(&dir)
                .with_extension("json.bad")
                .exists()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names() {
        assert_eq!(block_name("minecraft:oak_log[axis=y]"), "minecraft:oak_log");
        assert_eq!(block_name("minecraft:stone"), "minecraft:stone");
        assert_eq!(from_hex(&to_hex(&[0, 1, 254])), Some(vec![0, 1, 254]));
        assert_eq!(from_hex("abc"), None);
    }

    /// A `block_entities.json` that doesn't parse must not take the mod
    /// data saved on the world itself down with it.
    #[test]
    fn a_broken_file_keeps_the_saved_data() {
        let dir = std::env::temp_dir().join(format!("horizon-be-saved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut saved = crate::saved_data::SavedData::default();
        saved.set("m", "net", vec![7]).unwrap();
        saved.save(&dir).unwrap();
        std::fs::create_dir_all(dir.join("data/horizon")).unwrap();
        std::fs::write(BlockEntities::path(&dir), "nope").unwrap();
        let be = BlockEntities::load(&dir);
        assert_eq!(be.saved_data.get("m", "net"), Some(&[7][..]));
        // As before: the other stores wait for their own readable files.
        assert_eq!(be.of("m").count(), 0);
        assert!(
            BlockEntities::path(&dir)
                .with_extension("json.bad")
                .exists()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
