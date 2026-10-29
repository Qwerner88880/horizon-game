//! Capability `fluids`' tanks (docs/mods.md, "Fluids"): up to [`MAX_TANKS`]
//! tanks on each block entity that has any, kept by the mod owning that
//! block entity and shared with every mod. They live beside the block
//! entities ([`crate::BlockEntities::fluids`]), are lent and saved with them,
//! in their own file. Amounts are in millibuckets: a bucket is 1000.
use crate::block_entities::Key;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The file, from the world folder.
pub const FILE: &str = "data/horizon/fluids.json";
/// Tanks on one block entity.
pub const MAX_TANKS: usize = 8;
/// The most a tank may hold: far past anything a game makes, and inside the
/// range a double keeps whole.
pub const MAX_CAPACITY: u64 = 1 << 50;
/// What a bucket holds.
pub const BUCKET: u64 = 1000;

/// What a tank is made as: its limits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spec {
    pub capacity: u64,
    /// The most an `insert` puts in, and an `extract` takes out, per call.
    pub max_receive: u64,
    pub max_extract: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tank {
    /// `minecraft:water`; empty while the tank is.
    pub fluid: String,
    pub amount: u64,
    #[serde(flatten)]
    pub spec: Spec,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tanks {
    /// By position: the owning mod's id and its tanks.
    at: BTreeMap<Key, (String, Vec<Tank>)>,
    pub(crate) dirty: bool,
}

/// One block entity's tanks as the file holds them.
#[derive(Deserialize, Serialize)]
struct Saved {
    owner: String,
    dimension: String,
    pos: [i32; 3],
    tanks: Vec<Tank>,
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

/// Whether `fluid` is a fluid name: `namespace:path`, in the characters an
/// identifier has. The game has no registry of a mod's own fluids to check
/// against, so only the shape is.
pub fn fluid_name_ok(fluid: &str) -> bool {
    let plain = |t: &str| {
        !t.is_empty()
            && t.len() <= 64
            && t.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-/".contains(&b))
    };
    fluid
        .split_once(':')
        .is_some_and(|(ns, p)| plain(ns) && plain(p))
}

/// The fluid a bucket item holds, for the vanilla buckets.
pub fn fluid_of_bucket(item: &str) -> Option<&'static str> {
    match item {
        "minecraft:water_bucket" => Some("minecraft:water"),
        "minecraft:lava_bucket" => Some("minecraft:lava"),
        "minecraft:powder_snow_bucket" => Some("minecraft:powder_snow"),
        _ => None,
    }
}

/// The bucket item that holds `fluid`, for the vanilla fluids.
pub fn bucket_of_fluid(fluid: &str) -> Option<&'static str> {
    match fluid {
        "minecraft:water" => Some("minecraft:water_bucket"),
        "minecraft:lava" => Some("minecraft:lava_bucket"),
        "minecraft:powder_snow" => Some("minecraft:powder_snow_bucket"),
        _ => None,
    }
}

impl Tanks {
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
                let mut tanks = s.tanks;
                tanks.truncate(MAX_TANKS);
                for t in &mut tanks {
                    t.spec.capacity = t.spec.capacity.min(MAX_CAPACITY);
                    t.amount = t.amount.min(t.spec.capacity);
                    if t.amount == 0 || !fluid_name_ok(&t.fluid) {
                        t.amount = 0;
                        t.fluid.clear();
                    }
                }
                ((s.dimension, s.pos), (s.owner, tanks))
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
            .map(|((dimension, pos), (owner, tanks))| Saved {
                owner: owner.clone(),
                dimension: dimension.clone(),
                pos: *pos,
                tanks: tanks.clone(),
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

    /// The tanks at `key`, whichever mod keeps them.
    pub fn get(&self, key: &Key) -> Option<Vec<Tank>> {
        self.at.get(key).map(|(_, t)| t.clone())
    }

    /// Gives `owner`'s block entity at `key` tanks made as `specs`, or
    /// changes them: the tanks it keeps hold what they held up to their new
    /// capacity, the ones it loses lose it. The caller checks that `owner`
    /// has a block entity there.
    pub fn set_tanks(&mut self, owner: &str, key: Key, specs: &[Spec]) -> Result<(), String> {
        if specs.is_empty() || specs.len() > MAX_TANKS {
            return Err(format!("a block entity has 1 to {MAX_TANKS} tanks"));
        }
        if specs.iter().any(|s| s.capacity > MAX_CAPACITY) {
            return Err(format!("a tank holds at most {MAX_CAPACITY} mB"));
        }
        let old = match self.at.get(&key) {
            Some((o, _)) if o != owner => {
                return Err(format!("mod {o} keeps tanks at {:?} in {}", key.1, key.0));
            }
            Some((_, t)) => t.clone(),
            None => Vec::new(),
        };
        let tanks: Vec<Tank> = specs
            .iter()
            .enumerate()
            .map(|(i, spec)| {
                let mut tank = old.get(i).cloned().unwrap_or_default();
                tank.spec = *spec;
                tank.amount = tank.amount.min(spec.capacity);
                if tank.amount == 0 {
                    tank.fluid.clear();
                }
                tank
            })
            .collect();
        if self.at.get(&key).map(|(_, t)| t) != Some(&tanks) {
            self.at.insert(key, (owner.to_owned(), tanks));
            self.dirty = true;
        }
        Ok(())
    }

    /// `owner`'s own tank `tank` at `key`, for it to change.
    fn own(&mut self, owner: &str, key: &Key, tank: usize) -> Result<&mut Tank, String> {
        match self.at.get_mut(key) {
            Some((o, t)) if o == owner => t
                .get_mut(tank)
                .ok_or_else(|| format!("the block entity has no tank {tank}")),
            _ => Err(format!("no tanks of this mod at {:?} in {}", key.1, key.0)),
        }
    }

    /// Sets what `owner`'s own tank holds (a generator making steam, a
    /// machine using water): up to its capacity; `None` or 0 empties it.
    pub fn set(
        &mut self,
        owner: &str,
        key: &Key,
        tank: usize,
        fluid: Option<&str>,
        amount: u64,
    ) -> Result<(), String> {
        let fluid = fluid.filter(|_| amount > 0);
        if let Some(f) = fluid
            && !fluid_name_ok(f)
        {
            return Err(format!("{f:?} is not a fluid name (namespace:path)"));
        }
        let t = self.own(owner, key, tank)?;
        let amount = amount.min(t.spec.capacity);
        let fluid = fluid.filter(|_| amount > 0).unwrap_or("").to_owned();
        if t.fluid != fluid || t.amount != amount {
            t.fluid = fluid;
            t.amount = amount;
            self.dirty = true;
        }
        Ok(())
    }

    /// Puts up to `amount` mB of `fluid` into the tanks at `key`: into those
    /// that already hold it first, then into empty ones, each as far as its
    /// `max-receive` and room allow. What went in (0 where there are no
    /// tanks, or for a bad name). `simulate`: only says.
    pub fn insert(&mut self, key: &Key, fluid: &str, amount: u64, simulate: bool) -> u64 {
        if !fluid_name_ok(fluid) {
            return 0;
        }
        let Some((_, tanks)) = self.at.get_mut(key) else {
            return 0;
        };
        let mut left = amount;
        let mut moved = 0;
        // Tanks with the fluid, then the empty ones, in order.
        let order: Vec<usize> = (0..tanks.len())
            .filter(|&i| tanks[i].fluid == fluid)
            .chain((0..tanks.len()).filter(|&i| tanks[i].fluid.is_empty()))
            .collect();
        for i in order {
            if left == 0 {
                break;
            }
            let t = &mut tanks[i];
            let n = left.min(t.spec.max_receive).min(t.spec.capacity - t.amount);
            if n == 0 {
                continue;
            }
            if !simulate {
                t.fluid = fluid.to_owned();
                t.amount += n;
            }
            left -= n;
            moved += n;
        }
        if !simulate && moved > 0 {
            self.dirty = true;
        }
        moved
    }

    /// Takes up to `amount` mB from tank `tank` at `key`, as far as its
    /// `max-extract` and what it holds allow: the fluid and what came out.
    pub fn extract(
        &mut self,
        key: &Key,
        tank: usize,
        amount: u64,
        simulate: bool,
    ) -> Option<(String, u64)> {
        let (_, tanks) = self.at.get_mut(key)?;
        let t = tanks.get_mut(tank)?;
        let n = amount.min(t.spec.max_extract).min(t.amount);
        if n == 0 {
            return None;
        }
        let fluid = t.fluid.clone();
        if !simulate {
            t.amount -= n;
            if t.amount == 0 {
                t.fluid.clear();
            }
            self.dirty = true;
        }
        Some((fluid, n))
    }

    /// Moves up to `amount` mB from `owner`'s tank `tank` at `key` into the
    /// tanks on its six sides that take the fluid, shared evenly (the
    /// remainder to the first sides), each as far as it takes: what one
    /// leaves goes round again to those with room. What moved.
    pub fn push(
        &mut self,
        owner: &str,
        key: &Key,
        tank: usize,
        amount: u64,
    ) -> Result<u64, String> {
        let t = self.own(owner, key, tank)?;
        let (fluid, available) = (t.fluid.clone(), t.amount.min(amount));
        if fluid.is_empty() || available == 0 {
            return Ok(0);
        }
        let mut takers: Vec<Key> = sides(key)
            .into_iter()
            .filter(|k| self.insert(k, &fluid, 1, true) > 0)
            .collect();
        let mut moved = 0;
        // Each round fills at least one side or moves everything: six at most.
        while !takers.is_empty() && moved < available {
            let left = available - moved;
            let n = takers.len() as u64;
            let mut full = Vec::new();
            for (i, k) in takers.iter().enumerate() {
                let share = left / n + u64::from((i as u64) < left % n);
                let took = self.insert(k, &fluid, share, false);
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
        let t = self.own(owner, key, tank)?;
        t.amount -= moved;
        if t.amount == 0 {
            t.fluid.clear();
        }
        if moved > 0 {
            self.dirty = true;
        }
        Ok(moved)
    }

    /// Drops `owner`'s tanks at `key`, if it keeps any there.
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
    fn spec(capacity: u64, rx: u64, tx: u64) -> Spec {
        Spec {
            capacity,
            max_receive: rx,
            max_extract: tx,
        }
    }
    const WATER: &str = "minecraft:water";
    const LAVA: &str = "minecraft:lava";

    #[test]
    fn a_tank_takes_one_fluid_at_a_time() {
        let mut t = Tanks::default();
        t.set_tanks("m", key(0), &[spec(1000, 300, 200)]).unwrap();
        assert_eq!(t.insert(&key(0), WATER, 500, true), 300);
        assert_eq!(t.get(&key(0)).unwrap()[0].amount, 0);
        assert_eq!(t.insert(&key(0), WATER, 500, false), 300);
        // Lava does not mix in.
        assert_eq!(t.insert(&key(0), LAVA, 100, false), 0);
        assert_eq!(t.extract(&key(0), 0, 500, false), Some((WATER.into(), 200)));
        assert_eq!(t.get(&key(0)).unwrap()[0].amount, 100);
        assert_eq!(t.extract(&key(0), 0, 500, false), Some((WATER.into(), 100)));
        // Empty again: another fluid may come.
        assert_eq!(t.get(&key(0)).unwrap()[0].fluid, "");
        assert_eq!(t.insert(&key(0), LAVA, 100, false), 100);
        assert_eq!(t.extract(&key(0), 0, 1, true), Some((LAVA.into(), 1)));
        assert_eq!(t.extract(&key(0), 3, 1, true), None);
        assert_eq!(t.insert(&key(9), WATER, 5, false), 0);
        assert_eq!(t.insert(&key(0), "water", 5, false), 0);
    }

    #[test]
    fn several_tanks_fill_the_matching_one_first() {
        let mut t = Tanks::default();
        t.set_tanks("m", key(0), &[spec(100, 100, 100), spec(100, 100, 100)])
            .unwrap();
        t.set("m", &key(0), 1, Some(WATER), 10).unwrap();
        // Water goes to tank 1 (it has water) before the empty tank 0.
        assert_eq!(t.insert(&key(0), WATER, 150, false), 150);
        let got = t.get(&key(0)).unwrap();
        assert_eq!((got[1].amount, got[0].amount), (100, 60));
        assert_eq!(got[0].fluid, WATER);
        // Lava has no tank left.
        assert_eq!(t.insert(&key(0), LAVA, 10, false), 0);
    }

    #[test]
    fn only_the_owner_sets_and_resizes() {
        let mut t = Tanks::default();
        t.set_tanks("m", key(0), &[spec(100, 10, 10)]).unwrap();
        assert!(t.set("other", &key(0), 0, Some(WATER), 5).is_err());
        assert!(t.set_tanks("other", key(0), &[spec(1, 1, 1)]).is_err());
        assert!(t.set("m", &key(0), 1, Some(WATER), 5).is_err());
        assert!(t.set("m", &key(0), 0, Some("water"), 5).is_err());
        t.set("m", &key(0), 0, Some(WATER), 500).unwrap();
        assert_eq!(t.get(&key(0)).unwrap()[0].amount, 100);
        // A smaller tank keeps what fits; a second tank starts empty.
        t.set_tanks("m", key(0), &[spec(40, 10, 10), spec(5, 5, 5)])
            .unwrap();
        let got = t.get(&key(0)).unwrap();
        assert_eq!((got[0].amount, got[1].amount), (40, 0));
        // Zero empties and clears the fluid.
        t.set("m", &key(0), 0, Some(WATER), 0).unwrap();
        assert_eq!(t.get(&key(0)).unwrap()[0].fluid, "");
        assert!(t.set_tanks("m", key(0), &[]).is_err());
        assert!(t.set_tanks("m", key(0), &[spec(1, 1, 1); 9]).is_err());
        assert!(
            t.set_tanks("m", key(0), &[spec(MAX_CAPACITY + 1, 1, 1)])
                .is_err()
        );
    }

    #[test]
    fn push_shares_with_the_sides_that_take_the_fluid() {
        let mut t = Tanks::default();
        t.set_tanks("gen", key(0), &[spec(1000, 0, 0)]).unwrap();
        t.set("gen", &key(0), 0, Some(WATER), 101).unwrap();
        t.set_tanks("other", key(1), &[spec(1000, 1000, 0)])
            .unwrap();
        t.set_tanks("other", key(-1), &[spec(10, 1000, 0)]).unwrap();
        t.set_tanks("other", key(5), &[spec(1000, 1000, 0)])
            .unwrap();
        // A side holding lava does not take water.
        t.set_tanks("other", key(2), &[spec(1000, 1000, 0)])
            .unwrap();
        t.set("other", &key(2), 0, Some(LAVA), 1).unwrap();
        // -1 takes 10 of its 51; the 41 it leaves go round to +1.
        assert_eq!(t.push("gen", &key(0), 0, 1000).unwrap(), 101);
        assert_eq!(t.get(&key(1)).unwrap()[0].amount, 91);
        assert_eq!(t.get(&key(-1)).unwrap()[0].amount, 10);
        assert_eq!(t.get(&key(5)).unwrap()[0].amount, 0);
        assert_eq!(t.get(&key(0)).unwrap()[0].amount, 0);
        assert_eq!(t.get(&key(0)).unwrap()[0].fluid, "");
        // Nothing to push.
        assert_eq!(t.push("gen", &key(0), 0, 5).unwrap(), 0);
        assert!(t.push("other", &key(0), 0, 1).is_err());
        assert!(t.push("gen", &key(0), 4, 1).is_err());
    }

    #[test]
    fn buckets_and_names() {
        assert_eq!(fluid_of_bucket("minecraft:lava_bucket"), Some(LAVA));
        assert_eq!(bucket_of_fluid(WATER), Some("minecraft:water_bucket"));
        assert_eq!(fluid_of_bucket("minecraft:bucket"), None);
        assert_eq!(bucket_of_fluid("cells:plasma"), None);
        assert!(fluid_name_ok("cells:plasma"));
        assert!(!fluid_name_ok("Cells:Plasma"));
        assert!(!fluid_name_ok(""));
        assert!(!fluid_name_ok("minecraft:"));
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("horizon-fluids-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut t = Tanks::default();
        t.set_tanks("m", key(3), &[spec(100, 10, 10), spec(50, 5, 5)])
            .unwrap();
        t.set("m", &key(3), 1, Some(WATER), 42).unwrap();
        t.save(&dir).unwrap();
        let mut back = Tanks::load(&dir);
        let got = back.get(&key(3)).unwrap();
        assert_eq!((got[1].fluid.as_str(), got[1].amount), (WATER, 42));
        assert_eq!(got[0].amount, 0);
        back.remove("m", &key(3));
        assert_eq!(back.get(&key(3)), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
