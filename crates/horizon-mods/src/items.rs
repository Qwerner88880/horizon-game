//! Capability `items`' inventories (docs/mods.md, "Inventories"): slots on
//! block entities, kept by the mod owning the block entity and shared with
//! every mod, as energy is ([`crate::energy`]). They live beside the block
//! entities ([`crate::BlockEntities::items`]), are lent and saved with them,
//! in their own file.
use crate::block_entities::Key;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The file, from the world folder.
pub const FILE: &str = "data/horizon/items.json";
/// Slots one inventory may have.
pub const MAX_SLOTS: usize = 54;

/// A stack as mods see it: `components` in vanilla's item syntax
/// (`[custom_name={text:"x"}]`), empty for none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stack {
    pub item: String,
    pub count: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub components: String,
}

impl Stack {
    fn stacks_with(&self, other: &Stack) -> bool {
        self.item == other.item && self.components == other.components
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inventories {
    /// By position: the owning mod's id and its slots.
    at: BTreeMap<Key, (String, Vec<Option<Stack>>)>,
    pub(crate) dirty: bool,
}

#[derive(Deserialize, Serialize)]
struct Saved {
    owner: String,
    dimension: String,
    pos: [i32; 3],
    slots: Vec<Option<Stack>>,
}

impl Inventories {
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
                let mut slots = s.slots;
                slots.truncate(MAX_SLOTS);
                ((s.dimension, s.pos), (s.owner, slots))
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
            .map(|((dimension, pos), (owner, slots))| Saved {
                owner: owner.clone(),
                dimension: dimension.clone(),
                pos: *pos,
                slots: slots.clone(),
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

    pub fn get(&self, key: &Key) -> Option<&[Option<Stack>]> {
        self.at.get(key).map(|(_, s)| s.as_slice())
    }

    /// `owner`'s inventory at `key` with `slots` slots: made empty, or
    /// resized. The stacks of the slots it loses come back, to drop. The
    /// caller checks that `owner` has a block entity there.
    pub fn set_slots(&mut self, owner: &str, key: Key, slots: usize) -> Result<Vec<Stack>, String> {
        if slots > MAX_SLOTS {
            return Err(format!("{slots} slots: at most {MAX_SLOTS}"));
        }
        let entry = self
            .at
            .entry(key.clone())
            .or_insert_with(|| (owner.to_owned(), Vec::new()));
        if entry.0 != owner {
            return Err(format!(
                "mod {} keeps an inventory at {:?} in {}",
                entry.0, key.1, key.0
            ));
        }
        let lost = if entry.1.len() > slots {
            entry.1.split_off(slots).into_iter().flatten().collect()
        } else {
            Vec::new()
        };
        if entry.1.len() != slots {
            entry.1.resize(slots, None);
            self.dirty = true;
        }
        Ok(lost)
    }

    /// `owner`'s own slots at `key`, for it (or the game, for a player's
    /// clicks) to change.
    pub fn slots_mut(
        &mut self,
        owner: Option<&str>,
        key: &Key,
    ) -> Result<&mut Vec<Option<Stack>>, String> {
        self.dirty = true;
        match self.at.get_mut(key) {
            Some((o, s)) if owner.is_none_or(|w| w == o) => Ok(s),
            _ => Err(format!(
                "no inventory of this mod at {:?} in {}",
                key.1, key.0
            )),
        }
    }

    pub fn set(
        &mut self,
        owner: &str,
        key: &Key,
        slot: usize,
        stack: Option<Stack>,
    ) -> Result<(), String> {
        let slots = self.slots_mut(Some(owner), key)?;
        let n = slots.len();
        let s = slots
            .get_mut(slot)
            .ok_or_else(|| format!("slot {slot} of {n}"))?;
        *s = stack.filter(|s| s.count > 0);
        Ok(())
    }

    /// Puts as much of `stack` as fits (on equal stacks first, then empty
    /// slots, each up to `max` items): how many went in.
    pub fn insert(&mut self, key: &Key, stack: &Stack, max: u32, simulate: bool) -> u32 {
        let Some((_, slots)) = self.at.get_mut(key) else {
            return 0;
        };
        let mut left = stack.count;
        let mut work = slots.clone();
        for s in work.iter_mut().flatten() {
            if left > 0 && s.stacks_with(stack) && s.count < max {
                let n = left.min(max - s.count);
                s.count += n;
                left -= n;
            }
        }
        for s in work.iter_mut() {
            if left > 0 && s.is_none() {
                let n = left.min(max);
                *s = Some(Stack {
                    count: n,
                    ..stack.clone()
                });
                left -= n;
            }
        }
        let moved = stack.count - left;
        if !simulate && moved > 0 {
            *slots = work;
            self.dirty = true;
        }
        moved
    }

    /// Takes up to `count` items from slot `slot`: what came out.
    pub fn extract(&mut self, key: &Key, slot: usize, count: u32, simulate: bool) -> Option<Stack> {
        let (_, slots) = self.at.get_mut(key)?;
        let s = slots.get_mut(slot)?.as_mut()?;
        let n = count.min(s.count);
        if n == 0 {
            return None;
        }
        let out = Stack {
            count: n,
            ..s.clone()
        };
        if !simulate {
            s.count -= n;
            if s.count == 0 {
                slots[slot] = None;
            }
            self.dirty = true;
        }
        Some(out)
    }

    /// Removes `owner`'s inventory at `key`: its stacks, to drop.
    pub fn remove(&mut self, owner: &str, key: &Key) -> Vec<Stack> {
        if !self.at.get(key).is_some_and(|(o, _)| o == owner) {
            return Vec::new();
        }
        self.dirty = true;
        self.at
            .remove(key)
            .map(|(_, s)| s.into_iter().flatten().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(x: i32) -> Key {
        ("minecraft:overworld".into(), [x, 64, 0])
    }
    fn stack(item: &str, count: u32) -> Stack {
        Stack {
            item: item.into(),
            count,
            components: String::new(),
        }
    }

    #[test]
    fn insert_stacks_then_fills_empty_slots() {
        let mut inv = Inventories::default();
        assert!(inv.set_slots("m", key(0), 2).unwrap().is_empty());
        inv.set("m", &key(0), 1, Some(stack("minecraft:stone", 60)))
            .unwrap();
        assert_eq!(
            inv.insert(&key(0), &stack("minecraft:stone", 10), 64, true),
            10
        );
        assert_eq!(inv.get(&key(0)).unwrap()[1].as_ref().unwrap().count, 60);
        assert_eq!(
            inv.insert(&key(0), &stack("minecraft:stone", 80), 64, false),
            68
        );
        let slots = inv.get(&key(0)).unwrap();
        assert_eq!(slots[1].as_ref().unwrap().count, 64);
        assert_eq!(slots[0].as_ref().unwrap().count, 64);
        assert_eq!(
            inv.insert(&key(0), &stack("minecraft:dirt", 1), 64, false),
            0
        );
        assert_eq!(
            inv.insert(&key(9), &stack("minecraft:dirt", 1), 64, false),
            0
        );
    }

    #[test]
    fn extract_resize_and_owners() {
        let mut inv = Inventories::default();
        inv.set_slots("m", key(0), 3).unwrap();
        inv.set("m", &key(0), 2, Some(stack("minecraft:coal", 5)))
            .unwrap();
        assert_eq!(inv.extract(&key(0), 2, 2, false).unwrap().count, 2);
        assert_eq!(inv.extract(&key(0), 0, 2, false), None);
        assert!(inv.set("other", &key(0), 0, None).is_err());
        assert!(inv.set_slots("other", key(0), 1).is_err());
        assert!(inv.set_slots("m", key(0), MAX_SLOTS + 1).is_err());
        let lost = inv.set_slots("m", key(0), 1).unwrap();
        assert_eq!(lost, vec![stack("minecraft:coal", 3)]);
        inv.set("m", &key(0), 0, Some(stack("minecraft:coal", 1)))
            .unwrap();
        assert_eq!(inv.remove("m", &key(0)), vec![stack("minecraft:coal", 1)]);
        assert!(inv.get(&key(0)).is_none());
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("horizon-items-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut inv = Inventories::default();
        inv.set_slots("m", key(1), 2).unwrap();
        let named = Stack {
            components: "[custom_name={text:\"x\"}]".into(),
            ..stack("minecraft:stick", 3)
        };
        inv.set("m", &key(1), 0, Some(named.clone())).unwrap();
        inv.save(&dir).unwrap();
        let back = Inventories::load(&dir);
        assert_eq!(back.get(&key(1)).unwrap(), &[Some(named), None]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
