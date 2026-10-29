//! Capability `saved-data`' store (docs/mods.md, "World data"): the bytes
//! each server mod keeps on the world itself, by mod id and key — the
//! members of an energy network, who owns a claim, a list of teleports —
//! not tied to a block. Saved with the world beside `level.dat`, so it
//! travels with it, unlike the mod's own `storage` folder.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Bytes one key may hold.
pub const MAX_VALUE: usize = 64 * 1024;
/// Keys one mod may have.
pub const MAX_KEYS: usize = 256;
/// Bytes one mod may hold, across all its keys.
pub const MAX_TOTAL: usize = 1 << 20;
/// The file, from the world folder.
pub const FILE: &str = "data/horizon/saved_data.json";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedData {
    mods: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
    /// Changed since the last [`SavedData::save`].
    dirty: bool,
}

impl SavedData {
    pub fn path(world: &Path) -> PathBuf {
        world.join(FILE)
    }

    /// The world's saved mod data; none when it has no file yet. A file
    /// that doesn't parse is logged and kept aside, not lost.
    pub fn load(world: &Path) -> Self {
        let path = Self::path(world);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        let parsed: Result<BTreeMap<String, BTreeMap<String, String>>, _> =
            serde_json::from_str(&text);
        let Ok(saved) = parsed else {
            log::warn!("Horizon: {} does not parse: kept as .bad", path.display());
            let _ = std::fs::rename(&path, path.with_extension("json.bad"));
            return Self::default();
        };
        let mut out = Self::default();
        for (id, keys) in saved {
            let map = out.mods.entry(id).or_default();
            for (key, data) in keys {
                if let Some(bytes) =
                    crate::block_entities::from_hex(&data).filter(|d| d.len() <= MAX_VALUE)
                {
                    map.insert(key, bytes);
                }
            }
        }
        out
    }

    /// Writes the file when anything changed since the last save, through a
    /// temporary file so a crash mid-write keeps the old one.
    pub fn save(&mut self, world: &Path) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let saved: BTreeMap<&str, BTreeMap<&str, String>> = self
            .mods
            .iter()
            .filter(|(_, m)| !m.is_empty())
            .map(|(id, m)| {
                (
                    id.as_str(),
                    m.iter()
                        .map(|(key, data)| (key.as_str(), crate::block_entities::to_hex(data)))
                        .collect(),
                )
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

    pub fn get(&self, mod_id: &str, key: &str) -> Option<&[u8]> {
        self.mods.get(mod_id)?.get(key).map(Vec::as_slice)
    }

    /// Replaces the bytes at `key`, making it where the mod had none. An
    /// error for over [`MAX_VALUE`] bytes at one key, over [`MAX_KEYS`] keys
    /// or over [`MAX_TOTAL`] bytes for the whole mod.
    pub fn set(&mut self, mod_id: &str, key: &str, data: Vec<u8>) -> Result<(), String> {
        if data.len() > MAX_VALUE {
            return Err(format!(
                "{} bytes: at most {MAX_VALUE} at one key",
                data.len()
            ));
        }
        let map = self.mods.entry(mod_id.to_owned()).or_default();
        let held: usize = map.values().map(Vec::len).sum();
        let before = map.get(key).map_or(0, Vec::len);
        if !map.contains_key(key) && map.len() >= MAX_KEYS {
            return Err(format!("would hold over {MAX_KEYS} keys"));
        }
        if held - before + data.len() > MAX_TOTAL {
            return Err(format!("would hold over {} MiB of data", MAX_TOTAL >> 20));
        }
        if map.get(key) != Some(&data) {
            map.insert(key.to_owned(), data);
            self.dirty = true;
        }
        Ok(())
    }

    /// Removes `key` and its bytes; none was there is not an error.
    pub fn delete(&mut self, mod_id: &str, key: &str) {
        if self
            .mods
            .get_mut(mod_id)
            .is_some_and(|m| m.remove(key).is_some())
        {
            self.dirty = true;
        }
    }

    /// The mod's keys, in order.
    pub fn keys(&self, mod_id: &str) -> Vec<&str> {
        self.mods
            .get(mod_id)
            .into_iter()
            .flatten()
            .map(|(k, _)| k.as_str())
            .collect()
    }

    /// Every key and value of `mod_id`, in order, for tests.
    pub fn of(&self, mod_id: &str) -> Vec<(&str, &[u8])> {
        self.mods
            .get(mod_id)
            .into_iter()
            .flatten()
            .map(|(k, d)| (k.as_str(), d.as_slice()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_delete() {
        let mut data = SavedData::default();
        assert_eq!(data.get("m", "net"), None);
        assert_eq!(data.keys("m"), Vec::<&str>::new());
        data.set("m", "net", vec![1, 2]).unwrap();
        assert_eq!(data.get("m", "net"), Some(&[1, 2][..]));
        assert_eq!(data.get("other", "net"), None);
        // The same bytes again change nothing.
        data.set("m", "net", vec![1, 2]).unwrap();
        assert_eq!(data.keys("m"), ["net"]);
        data.delete("m", "net");
        data.delete("m", "net");
        assert_eq!(data.get("m", "net"), None);
        assert_eq!(data.keys("m"), Vec::<&str>::new());
        // Keys are ordered, and one mod's keys are not another's.
        data.set("m", "b", vec![]).unwrap();
        data.set("m", "a", vec![]).unwrap();
        assert_eq!(data.keys("m"), ["a", "b"]);
        assert_eq!(data.keys("n"), Vec::<&str>::new());
    }

    #[test]
    fn limits() {
        let mut data = SavedData::default();
        assert!(data.set("m", "big", vec![0; MAX_VALUE]).is_ok());
        assert!(data.set("m", "big", vec![0; MAX_VALUE + 1]).is_err());
        for i in 0..MAX_KEYS - 1 {
            data.set("m", &format!("k{i}"), Vec::new()).unwrap();
        }
        assert!(data.set("m", "one more", Vec::new()).is_err());
        // Replacing an existing key takes no new key.
        assert!(data.set("m", "k0", vec![0; 16]).is_ok());
        // One mebibyte for the whole mod, across its keys: the biggest
        // value fits sixteen times over, exactly, and no byte more.
        let mut data = SavedData::default();
        for i in 0..MAX_TOTAL / MAX_VALUE {
            assert!(data.set("m", &format!("k{i}"), vec![0; MAX_VALUE]).is_ok());
        }
        assert!(data.set("m", "one more", vec![1]).is_err());
        // Replacing a key with a same-size value stays within it.
        assert!(data.set("m", "k0", vec![0; MAX_VALUE]).is_ok());
        // Another mod's data is not this one's quota.
        assert!(data.set("n", "k0", vec![0; MAX_VALUE]).is_ok());
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("horizon-saved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut data = SavedData::default();
        data.set("m", "net", vec![0, 255, 16]).unwrap();
        data.set("m", "gone", vec![1]).unwrap();
        data.set("n", "own", Vec::new()).unwrap();
        data.delete("m", "gone");
        data.save(&dir).unwrap();
        let back = SavedData::load(&dir);
        assert_eq!(back.get("m", "net"), Some(&[0, 255, 16][..]));
        assert_eq!(back.get("m", "gone"), None);
        assert_eq!(back.get("n", "own"), Some(&[][..]));
        assert_eq!(back.keys("m"), ["net"]);
        // Deleting and saving again keeps the deletion.
        let mut back = back;
        back.delete("m", "net");
        back.save(&dir).unwrap();
        assert_eq!(SavedData::load(&dir).of("m"), Vec::<(&str, &[u8])>::new());
        std::fs::write(SavedData::path(&dir), "nope").unwrap();
        assert_eq!(SavedData::load(&dir).of("m"), Vec::<(&str, &[u8])>::new());
        assert!(SavedData::path(&dir).with_extension("json.bad").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_clean_store_saves_nothing() {
        let dir = std::env::temp_dir().join(format!("horizon-saved-kept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut data = SavedData::default();
        data.set("m", "net", vec![1]).unwrap();
        data.save(&dir).unwrap();
        // A store nothing changed in writes no file.
        let mut again = SavedData::load(&dir);
        std::fs::remove_file(SavedData::path(&dir)).unwrap();
        again.save(&dir).unwrap();
        // Setting the same bytes again keeps it clean.
        again.set("m", "net", vec![1]).unwrap();
        again.save(&dir).unwrap();
        assert!(!SavedData::path(&dir).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
