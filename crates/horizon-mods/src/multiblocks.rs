//! Capability `multiblocks`' table (docs/mods.md, "Multiblocks"): the
//! structures of the server mods' own multiblocks, read once from each mod's
//! `data/<id>/multiblock/` files — in its own package ([`files_in_package`])
//! and in a content mod's carry of the same id ([`files_in_dir`]) — before
//! the mods start, as `machine-recipes` are. Content, not state: nothing here
//! is saved with the world; the files are read again at every start. The
//! watches ([`Watches`]) are state: they live beside the block entities
//! ([`crate::BlockEntities::multiblocks`]) and are saved with them.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, io::Read, path::Path};

/// Structures one mod may carry; the rest are skipped, said so in the log.
pub const MAX_PER_MOD: usize = 64;
/// One template file's biggest size.
pub const MAX_FILE: u64 = 16 << 10;
/// A shape's biggest side, in blocks.
pub const MAX_SIDE: u32 = 32;
/// Checked cells one shape may have (the `any` cells are not checked).
pub const MAX_CELLS: usize = 4096;
/// Blocks that may fit one cell (any of them).
pub const MAX_BLOCKS: usize = 16;
/// Mismatches `check` answers with, at most.
pub const MISMATCHES: usize = 16;
/// Watches one mod may keep.
pub const MAX_WATCHES: usize = 256;

/// One checked cell of a shape: its offset from the anchor and the blocks
/// that fit it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub dx: i32,
    pub dy: i32,
    pub dz: i32,
    /// Any of these block ids fits; `minecraft:air` names a cell that must
    /// be empty. Block states (properties) are not matched.
    pub blocks: Vec<String>,
}

/// One structure of a mod, as its file says.
#[derive(Clone, Debug, PartialEq)]
pub struct Structure {
    /// `<mod id>:<path>`: the file's place under `data/<id>/multiblock/`.
    pub id: String,
    /// The shape's size in blocks: cells along x, layers along y, rows
    /// along z.
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    /// The shape's cell the anchor sits on: the mod's block entity whose
    /// position a check takes, as `[ax, ay, az]` from the shape's corner.
    pub anchor: [u32; 3],
    /// The shape's checked cells (`any` cells are left out), in their
    /// offsets' order from the anchor.
    pub cells: Vec<Cell>,
    /// The file's whole JSON text, for the fields only the mod knows.
    pub data: String,
}

/// One cell of a structure the world doesn't match, as a check answers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    /// Where in the world.
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// The block the template takes there, as `minecraft:iron_block`.
    pub expected: String,
    /// What is there, as `minecraft:oak_log[axis=y]`.
    pub found: String,
}

/// Every server mod's structures, by mod id: built once, before the mods
/// start, and given to [`crate::Mods::load_for`], so a mod's `init` already
/// reads its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Templates {
    by_mod: BTreeMap<String, Vec<Structure>>,
}

/// The `data/<id>/multiblock/**.json` files under `root`, as
/// `(structure id, JSON text)` with the id `<id>:<path>`: the content layer's
/// carry of a mod's structures, as `horizon-forge convert` lays it down. A
/// file over [`MAX_FILE`], or one that is not UTF-8, is skipped with a line
/// in the log. Only the mod's own id is read: a server mod's manifest may
/// list no other namespace.
pub fn files_in_dir(root: &Path, id: &str) -> Vec<(String, String)> {
    let mut files = Vec::new();
    let tree = root.join("data").join(id).join("multiblock");
    read_tree(&tree, &tree, &mut files);
    files
        .into_iter()
        .filter_map(|(name, text)| Some((structure_id(id, &name)?, text)))
        .collect()
}

/// [`files_in_dir`] over the mod at `path`: a mod folder's
/// `data/<id>/multiblock/` tree, or the same entries of its `.hmod`. A file
/// over [`MAX_FILE`], or one that is not UTF-8, is skipped with a line in
/// the log.
pub fn files_in_package(path: &Path, id: &str) -> Vec<(String, String)> {
    if path.is_dir() {
        return files_in_dir(path, id);
    }
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return Vec::new();
    };
    let prefix = format!("data/{id}/multiblock/");
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let Ok(entry) = zip.by_index(i) else {
            continue;
        };
        let name = entry.name().to_owned();
        let Some(rest) = name.strip_prefix(&prefix) else {
            continue;
        };
        if entry.is_dir() || !name.ends_with(".json") {
            continue;
        }
        let mut bytes = Vec::new();
        if entry.take(MAX_FILE + 1).read_to_end(&mut bytes).is_err() {
            log::warn!("Horizon: multiblock {name} does not read: skipped");
            continue;
        }
        if bytes.len() as u64 > MAX_FILE {
            log::warn!(
                "Horizon: multiblock {name} is over {} KiB: skipped",
                MAX_FILE >> 10
            );
            continue;
        }
        match String::from_utf8(bytes) {
            Ok(text) => {
                if let Some(sid) = structure_id(id, rest) {
                    out.push((sid, text));
                }
            }
            Err(_) => log::warn!("Horizon: multiblock {name} is not UTF-8: skipped"),
        }
    }
    out
}

/// The `.json` files under `dir`, recursively, by their `/`-separated name
/// under `root` (a folder's subfolders are allowed:
/// `multiblock/tier/core.json`).
fn read_tree(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            read_tree(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            let name = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_FILE) {
                log::warn!(
                    "Horizon: multiblock {} is over {} KiB: skipped",
                    path.display(),
                    MAX_FILE >> 10
                );
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(text) => out.push((name, text)),
                Err(_) => log::warn!(
                    "Horizon: multiblock {} does not read as text: skipped",
                    path.display()
                ),
            }
        }
    }
}

/// The structure id of a file at `path` under `data/<id>/multiblock/`:
/// `<id>:<path>` without the `.json`, or None for a name that is no plain
/// resource location (the same characters the content fence allows).
fn structure_id(id: &str, path: &str) -> Option<String> {
    let plain = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-/".contains(&b))
    };
    let path = path.strip_suffix(".json")?;
    path.split('/').all(plain).then(|| format!("{id}:{path}"))
}

/// Parses structure `files` — `(structure id, JSON text)`, as
/// [`files_in_package`] and [`files_in_dir`] read them — into one table for
/// every mod: a structure id names its mod before the `:`, and a later file
/// with the same id wins, so a mod's package beats a content mod's carry of
/// it. A file that does not parse, or whose shape is not the template syntax,
/// is skipped with a line in the log: one bad file keeps neither the mod nor
/// the other structures from loading.
pub fn build(files: &[(String, String)]) -> Templates {
    let mut parsed: BTreeMap<&str, Structure> = BTreeMap::new();
    for (id, text) in files {
        if let Some(structure) = parse(id, text) {
            parsed.insert(id, structure);
        }
    }
    let mut by_mod: BTreeMap<String, Vec<Structure>> = BTreeMap::new();
    let mut over = Vec::new();
    for (_, structure) in parsed {
        let m = structure
            .id
            .split(':')
            .next()
            .unwrap_or_default()
            .to_owned();
        let list = by_mod.entry(m.clone()).or_default();
        if list.len() < MAX_PER_MOD {
            list.push(structure);
        } else if over.is_empty() || *over.last().expect("checked") != m {
            over.push(m);
        }
    }
    for m in over {
        log::warn!(
            "Horizon: mod {m} carries over {MAX_PER_MOD} multiblock structures: the rest are skipped"
        );
    }
    Templates { by_mod }
}

impl Templates {
    /// The structures of mod `id`, in their ids' order.
    pub fn of_mod(&self, id: &str) -> &[Structure] {
        self.by_mod.get(id).map_or(&[], Vec::as_slice)
    }

    /// Every mod with structures (its id), in order.
    pub fn mods(&self) -> impl Iterator<Item = &str> {
        self.by_mod.keys().map(String::as_str)
    }
}

/// One template file: None, with a line in the log, when it does not parse
/// or the shape is not the template syntax.
///
/// The file names its shape in `layers`: one string per row, one row array
/// per layer from the bottom up — `depth` rows of `width` characters each,
/// x along the string and z down the rows. Each character names its cell's
/// entry in `palette`: a block id, a list of them (any fits), `"air"` (the
/// cell must be empty) or `"any"` (not checked). A space is always air.
/// `anchor` is the shape's cell the mod's block entity sits on (`[0, 0, 0]`
/// by default).
fn parse(id: &str, text: &str) -> Option<Structure> {
    let skipped = |why: String| {
        log::warn!("Horizon: multiblock {id} skipped: {why}");
        None
    };
    let Ok(file) = serde_json::from_str::<Value>(text) else {
        return skipped("does not parse".into());
    };
    let Some(layers) = file.get("layers").and_then(Value::as_array) else {
        return skipped("has no layers".into());
    };
    if layers.is_empty() || layers.len() > MAX_SIDE as usize {
        return skipped(format!("has {} layers: 1 to {MAX_SIDE}", layers.len()));
    }
    fn rows_of(layer: &Value) -> Result<Vec<&str>, String> {
        let Some(rows) = layer.as_array() else {
            return Err("a layer is not a list of rows".into());
        };
        if rows.is_empty() || rows.len() > MAX_SIDE as usize {
            return Err(format!("has {} rows: 1 to {MAX_SIDE}", rows.len()));
        }
        rows.iter()
            .map(|r| r.as_str().ok_or_else(|| "a row is not text".to_string()))
            .collect()
    }
    let first = match rows_of(&layers[0]) {
        Ok(rows) => rows,
        Err(why) => return skipped(why),
    };
    let (width, depth) = (first[0].len(), first.len());
    if width == 0 || width > MAX_SIDE as usize {
        return skipped(format!("rows are {width} wide: 1 to {MAX_SIDE}"));
    }
    let mut rows = Vec::with_capacity(layers.len());
    rows.push(first);
    for layer in &layers[1..] {
        let rows_l = match rows_of(layer) {
            Ok(rows) => rows,
            Err(why) => return skipped(why),
        };
        if rows_l.len() != depth || rows_l.iter().any(|r| r.len() != width) {
            return skipped("a layer is not the shape's size".into());
        }
        rows.push(rows_l);
    }
    let anchor = match file.get("anchor") {
        None => [0, 0, 0],
        Some(a) => {
            let Some(list) = a.as_array().filter(|a| a.len() == 3) else {
                return skipped("anchor is not [x, y, z]".into());
            };
            let Some(a) = list
                .iter()
                .map(|v| {
                    v.as_u64()
                        .filter(|n| *n <= u32::MAX as u64)
                        .map(|n| n as u32)
                })
                .collect::<Option<Vec<u32>>>()
            else {
                return skipped("anchor is not [x, y, z]".into());
            };
            [a[0], a[1], a[2]]
        }
    };
    if anchor[0] >= width as u32 || anchor[1] >= rows.len() as u32 || anchor[2] >= depth as u32 {
        return skipped(format!("anchor {anchor:?} is outside the shape"));
    }
    let Some(palette) = file.get("palette").and_then(Value::as_object) else {
        return skipped("has no palette".into());
    };
    for key in palette.keys() {
        let Some(c) = key.chars().next() else {
            return skipped("names an empty character in the palette".into());
        };
        if key.chars().count() != 1 {
            return skipped(format!("'{key}' is not one character in the palette"));
        }
        if c == ' ' {
            return skipped("names ' ' in the palette: a space is always air".into());
        }
    }
    // Every entry's form is checked now, also the ones no layer names: a
    // wrong block id is caught the day the file is read, not the day it fits.
    for value in palette.values() {
        if let Err(why) = entry(value) {
            return skipped(why);
        }
    }
    let mut cells = Vec::new();
    for (y, layer) in rows.iter().enumerate() {
        for (z, row) in layer.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                let blocks = if c == ' ' {
                    vec!["minecraft:air".to_string()]
                } else {
                    let Some(value) = palette.get(&c.to_string()) else {
                        return skipped(format!("'{c}' is named but not in the palette"));
                    };
                    match entry(value) {
                        Ok(None) => continue, // `any`: not checked.
                        Ok(Some(blocks)) => blocks,
                        Err(why) => return skipped(why),
                    }
                };
                cells.push(Cell {
                    dx: x as i32 - anchor[0] as i32,
                    dy: y as i32 - anchor[1] as i32,
                    dz: z as i32 - anchor[2] as i32,
                    blocks,
                });
            }
        }
    }
    if cells.len() > MAX_CELLS {
        return skipped(format!(
            "has {} checked cells: at most {MAX_CELLS}",
            cells.len()
        ));
    }
    cells.sort_by_key(|c| (c.dx, c.dy, c.dz));
    Some(Structure {
        id: id.to_owned(),
        width: width as u32,
        height: rows.len() as u32,
        depth: depth as u32,
        anchor,
        cells,
        data: text.to_owned(),
    })
}

/// A palette entry: `Ok(None)` for `"any"`, the blocks that fit the cell
/// else. `"air"` and a space name a cell that must be empty
/// (`minecraft:air`).
fn entry(value: &Value) -> Result<Option<Vec<String>>, String> {
    let block = |s: &str| -> Result<String, String> {
        if s == "air" {
            return Ok("minecraft:air".to_string());
        }
        if s.is_empty() || s.contains('[') {
            return Err(format!("{s:?} is not a block id (states are not matched)"));
        }
        Ok(s.to_string())
    };
    match value {
        Value::String(s) if s == "any" => Ok(None),
        Value::String(s) => Ok(Some(vec![block(s)?])),
        Value::Array(list) => {
            if list.is_empty() || list.len() > MAX_BLOCKS {
                return Err(format!("lists {} blocks: 1 to {MAX_BLOCKS}", list.len()));
            }
            let blocks = list
                .iter()
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| "a palette block is not text".to_string())
                        .and_then(block)
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Some(blocks))
        }
        _ => Err("a palette entry is a block, a list of them, \"air\" or \"any\"".into()),
    }
}

/// A cell's offset turned by `quarter` quarter turns clockwise looked at
/// from above; y offsets don't turn.
fn turn(dx: i32, dz: i32, quarter: u32) -> (i32, i32) {
    match quarter % 4 {
        0 => (dx, dz),
        1 => (dz, -dx),
        2 => (-dx, -dz),
        _ => (-dz, dx),
    }
}

/// Matches `structure` against the world with its anchor at `[x, y, z]`,
/// turned by `quarter` quarter turns: `block` answers the block state at a
/// position (`None` where no chunk is loaded), compared by block id only —
/// properties are not matched. `Ok(None)`: the structure is whole;
/// `Ok(Some(..))`: its first mismatches, at most [`MISMATCHES`], in the
/// cells' order. Err for a `quarter` past 3 or a position whose chunk is not
/// loaded. Only reads: it changes nothing.
pub fn check_cells(
    structure: &Structure,
    at: [i32; 3],
    quarter: u32,
    block: &dyn Fn(i32, i32, i32) -> Option<String>,
) -> Result<Option<Vec<Mismatch>>, String> {
    if quarter > 3 {
        return Err(format!("{quarter} quarter turns: 0 to 3"));
    }
    let mut mismatches = Vec::new();
    for cell in &structure.cells {
        let (dx, dz) = turn(cell.dx, cell.dz, quarter);
        let (x, y, z) = (at[0] + dx, at[1] + cell.dy, at[2] + dz);
        let Some(found) = block(x, y, z) else {
            return Err("a chunk there is not loaded".into());
        };
        let name = crate::block_entities::block_name(&found);
        if mismatches.len() < MISMATCHES && !cell.blocks.iter().any(|b| b == name) {
            mismatches.push(Mismatch {
                x,
                y,
                z,
                expected: cell.blocks.join(" or "),
                found,
            });
        }
    }
    Ok((!mismatches.is_empty()).then_some(mismatches))
}

/// One followed copy of a structure, as the last check saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Watch {
    /// The quarter turn it is whole at.
    pub quarter: u32,
    /// Whether it held at the last check.
    pub formed: bool,
    /// Whether the mod has heard `formed` since it last came together (a
    /// whole structure watched into place is reported by the next check
    /// pass, not by the `watch` call that saw it).
    pub told: bool,
}

impl Watch {
    /// Never checked, never told: what a fresh watch starts as.
    pub const UNTOLD: Watch = Watch {
        quarter: 0,
        formed: false,
        told: false,
    };
}

/// Capability `multiblocks`' watches: the copies of its structures the game
/// follows around the mod's block entities, saved with the world
/// (`data/horizon/multiblocks.json`). A watch goes with its anchor: when the
/// block entity goes, the watch does too.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Watches {
    by_mod: BTreeMap<String, BTreeMap<Key, BTreeMap<String, Watch>>>,
    /// How far the last check pass got, in the mod's watches' order; starts
    /// the next pass where this one stopped. Not saved.
    pub(crate) cursor: usize,
    pub(crate) dirty: bool,
}

/// The file, from the world folder.
pub const WATCH_FILE: &str = "data/horizon/multiblocks.json";

/// One watch as the file holds it.
#[derive(Deserialize, Serialize)]
struct Saved {
    dimension: String,
    pos: [i32; 3],
    /// The followed structure, as `cells:core`.
    structure: String,
    quarter: u32,
    formed: bool,
    told: bool,
}

/// A watch's place: the anchor's dimension and position, as
/// [`crate::block_entities::Key`].
pub type Key = crate::block_entities::Key;

impl Watches {
    pub fn load(world: &Path) -> Self {
        let path = world.join(WATCH_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        let Ok(saved) = serde_json::from_str::<BTreeMap<String, Vec<Saved>>>(&text) else {
            log::warn!("Horizon: {} does not parse: kept as .bad", path.display());
            let _ = std::fs::rename(&path, path.with_extension("json.bad"));
            return Self::default();
        };
        let mut by_mod = BTreeMap::new();
        for (id, watches) in saved {
            let map: &mut BTreeMap<Key, BTreeMap<String, Watch>> = by_mod.entry(id).or_default();
            for w in watches {
                map.entry((w.dimension, w.pos)).or_default().insert(
                    w.structure,
                    Watch {
                        quarter: w.quarter.min(3),
                        formed: w.formed,
                        told: w.told,
                    },
                );
            }
        }
        Self {
            by_mod,
            ..Self::default()
        }
    }

    pub fn save(&mut self, world: &Path) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let saved: BTreeMap<&str, Vec<Saved>> = self
            .by_mod
            .iter()
            .filter(|(_, m)| !m.is_empty())
            .map(|(id, m)| {
                let watches = m
                    .iter()
                    .flat_map(|((dimension, pos), structures)| {
                        structures.iter().map(|(structure, w)| Saved {
                            dimension: dimension.clone(),
                            pos: *pos,
                            structure: structure.clone(),
                            quarter: w.quarter,
                            formed: w.formed,
                            told: w.told,
                        })
                    })
                    .collect();
                (id.as_str(), watches)
            })
            .collect();
        let path = world.join(WATCH_FILE);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(&saved)?)?;
        std::fs::rename(&tmp, &path)?;
        self.dirty = false;
        Ok(())
    }

    /// Starts following, or retells, the copy of `structure` whose anchor is
    /// the mod's block entity at `key`. Err over [`MAX_WATCHES`] structures
    /// watched by the mod.
    pub fn watch(&mut self, mod_id: &str, key: Key, structure: &str) -> Result<(), String> {
        if self.get(mod_id, &key, structure).is_none() && self.count(mod_id) >= MAX_WATCHES {
            return Err(format!("watches over {MAX_WATCHES} structures of this mod"));
        }
        // A watch that comes back keeps what the last check saw.
        let anchors = self
            .by_mod
            .entry(mod_id.to_owned())
            .or_default()
            .entry(key)
            .or_default();
        if !anchors.contains_key(structure) {
            anchors.insert(structure.to_owned(), Watch::UNTOLD);
            self.dirty = true;
        }
        Ok(())
    }

    /// The watch of `structure` at `key`, if the mod keeps one.
    pub fn get(&self, mod_id: &str, key: &Key, structure: &str) -> Option<Watch> {
        self.by_mod.get(mod_id)?.get(key)?.get(structure).copied()
    }

    /// Writes what a check saw: the pass's, or `watch`'s where the world
    /// was lent. A watch that is no longer whole forgets it was told, so
    /// the next coming together is news again.
    pub fn set(&mut self, mod_id: &str, key: &Key, structure: &str, watch: Watch) {
        if let Some(w) = self
            .by_mod
            .get_mut(mod_id)
            .and_then(|m| m.get_mut(key))
            .and_then(|a| a.get_mut(structure))
            && *w != watch
        {
            *w = watch;
            self.dirty = true;
        }
    }

    /// Every watch at `key` goes, with its anchor's block entity: the
    /// watches removed, by structure id.
    pub fn remove_anchor(&mut self, mod_id: &str, key: &Key) -> Vec<(String, Watch)> {
        let gone = self
            .by_mod
            .get_mut(mod_id)
            .and_then(|m| m.remove(key))
            .map(|structures| structures.into_iter().collect::<Vec<(String, Watch)>>())
            .unwrap_or_default();
        if !gone.is_empty() {
            self.dirty = true;
        }
        gone
    }

    /// The mod's watches, in `(key, structure)` order: the pass walks them
    /// from its cursor.
    pub fn of(&self, mod_id: &str) -> impl Iterator<Item = (&Key, &str, Watch)> {
        self.by_mod
            .get(mod_id)
            .into_iter()
            .flatten()
            .flat_map(|(key, structures)| {
                structures
                    .iter()
                    .map(move |(structure, w)| (key, structure.as_str(), *w))
            })
    }

    /// How many structures the mod watches.
    pub fn count(&self, mod_id: &str) -> usize {
        self.by_mod
            .get(mod_id)
            .map_or(0, |m| m.values().map(BTreeMap::len).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The energy core of the cells example, as its file spells it.
    const CORE: &str = r####"{
        "anchor": [1, 1, 1],
        "palette": { "#": "minecraft:iron_block", "C": "minecraft:lodestone",
                     ".": "air", "?": "any" },
        "layers": [ ["###", "#.#", "###"], ["###", "#C#", "###"], ["###", "#.#", "###"] ]
    }"####;

    fn core() -> Structure {
        build(&[("cells:core".into(), CORE.into())])
            .of_mod("cells")
            .first()
            .cloned()
            .expect("parses")
    }

    /// The world as `check_cells` reads it: a map of states, air elsewhere.
    fn world<'a>(
        of: &'a [(i32, i32, i32, &'a str)],
    ) -> impl Fn(i32, i32, i32) -> Option<String> + 'a {
        move |x, y, z| {
            Some(
                of.iter()
                    .find(|&&(a, b, c, _)| (a, b, c) == (x, y, z))
                    .map_or_else(
                        || "minecraft:air".to_string(),
                        |&(_, _, _, s)| s.to_string(),
                    ),
            )
        }
    }

    #[test]
    fn parses_the_shape() {
        let s = core();
        assert_eq!(s.id, "cells:core");
        assert_eq!((s.width, s.height, s.depth), (3, 3, 3));
        assert_eq!(s.anchor, [1, 1, 1]);
        // Every cell is checked (`?` is named but never used): 24 iron, the
        // anchor and two air cells, sorted by their offsets from the anchor.
        assert_eq!(s.cells.len(), 27);
        assert_eq!(
            s.cells[0],
            Cell {
                dx: -1,
                dy: -1,
                dz: -1,
                blocks: vec!["minecraft:iron_block".into()],
            }
        );
        let anchor = s
            .cells
            .iter()
            .find(|c| c.blocks == ["minecraft:lodestone".to_string()])
            .unwrap();
        assert_eq!((anchor.dx, anchor.dy, anchor.dz), (0, 0, 0));
        assert_eq!(s.data, CORE);
    }

    #[test]
    fn a_default_anchor_sits_at_the_corner() {
        let s = build(&[(
            "m:s".into(),
            r####"{"palette": {"#": "minecraft:stone"}, "layers": [["#"]]}"####.into(),
        )])
        .of_mod("m")
        .first()
        .cloned()
        .unwrap();
        assert_eq!(s.anchor, [0, 0, 0]);
        assert_eq!(
            s.cells,
            [Cell {
                dx: 0,
                dy: 0,
                dz: 0,
                blocks: vec!["minecraft:stone".into()],
            }]
        );
        // A space and "air" name a cell that must be empty; a list holds
        // the blocks, any of which fits.
        let s = build(&[(
            "m:s".into(),
            r####"{"palette": {"#": ["minecraft:stone", "minecraft:iron_block"], ".": "air"},
                "layers": [["# ", " #"]]}"####
                .into(),
        )])
        .of_mod("m")
        .first()
        .cloned()
        .unwrap();
        assert_eq!(s.cells.len(), 4);
        let air = s.cells.iter().find(|c| c.dx == 1).unwrap();
        assert_eq!(air.blocks, ["minecraft:air".to_string()]);
    }

    #[test]
    fn any_cells_are_not_checked() {
        let s = build(&[(
            "m:s".into(),
            r####"{"palette": {"#": "minecraft:stone", "?": "any"},
                "layers": [["?#", "##"]]}"####
                .into(),
        )])
        .of_mod("m")
        .first()
        .cloned()
        .unwrap();
        // The `?` cell is left out: three checked cells, sorted by offset.
        let offsets: Vec<(i32, i32, i32)> = s.cells.iter().map(|c| (c.dx, c.dy, c.dz)).collect();
        assert_eq!(offsets, [(0, 0, 1), (1, 0, 0), (1, 0, 1)]);
    }

    #[test]
    fn broken_files_are_skipped_not_fatal() {
        let files = [
            ("m:bad".to_string(), "{ not json".to_string()),
            ("m:nolayers".to_string(), r#"{"palette": {}}"#.to_string()),
            (
                "m:nopalette".to_string(),
                r####"{"layers": [["#"]]}"####.to_string(),
            ),
            (
                "m:unknown".to_string(),
                r####"{"palette": {}, "layers": [["#"]]}"####.to_string(),
            ),
            (
                "m:anchor".to_string(),
                r####"{"anchor": [3, 0, 0], "palette": {"#": "minecraft:stone"},
                    "layers": [["##"]]}"####
                    .to_string(),
            ),
            (
                "m:state".to_string(),
                r####"{"palette": {"#": "minecraft:oak_log[axis=y]"}, "layers": [["#"]]}"####
                    .to_string(),
            ),
            (
                "m:space".to_string(),
                r####"{"palette": {" ": "minecraft:stone"}, "layers": [["#"]]}"####.to_string(),
            ),
            (
                "m:emptylist".to_string(),
                r####"{"palette": {"#": []}, "layers": [["#"]]}"####.to_string(),
            ),
            (
                "m:ragged".to_string(),
                r####"{"palette": {"#": "minecraft:stone"}, "layers": [["##"], ["#"]]}"####
                    .to_string(),
            ),
            (
                "m:good".to_string(),
                r####"{"palette": {"#": "minecraft:stone"}, "layers": [["#"]]}"####.to_string(),
            ),
        ];
        let table = build(&files);
        assert_eq!(table.mods().collect::<Vec<_>>(), ["m"]);
        let ids: Vec<&str> = table.of_mod("m").iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["m:good"]);
        // A wrong palette entry skips the file even when no layer names it.
        assert!(
            build(&[(
                "m:unused".into(),
                r####"{"palette": {"#": "minecraft:stone", "?": 3}, "layers": [["#"]]}"####.into()
            )])
            .of_mod("m")
            .is_empty()
        );
    }

    #[test]
    fn shapes_and_mods_carry_at_most_their_limits() {
        // A shape of MAX_CELLS checked cells: 32 x 32 over 4 layers. One
        // more layer is over the cap, a side over 32 is not the syntax.
        let row = format!("{:?}", "#".repeat(MAX_SIDE as usize));
        let layer = format!("[{}]", vec![row; MAX_SIDE as usize].join(", "));
        let layers = |n: usize| vec![layer.clone(); n].join(", ");
        let shape = |n: usize| {
            format!(
                r####"{{"palette": {{"#": "minecraft:stone"}}, "layers": [{}]}}"####,
                layers(n)
            )
        };
        let cells_per_layer = MAX_SIDE as usize * MAX_SIDE as usize;
        assert_eq!(
            build(&[("m:cap".into(), shape(MAX_CELLS / cells_per_layer))])
                .of_mod("m")
                .len(),
            1
        );
        assert!(
            build(&[("m:over".into(), shape(MAX_CELLS / cells_per_layer + 1))])
                .of_mod("m")
                .is_empty()
        );
        let wide = format!(
            r####"{{"palette": {{"#": "minecraft:stone"}}, "layers": [[{:?}]]}}"####,
            "#".repeat(MAX_SIDE as usize + 1)
        );
        assert!(build(&[("m:wide".into(), wide)]).of_mod("m").is_empty());
        // At most 64 structures a mod, counted separately per mod.
        let one = r####"{"palette": {"#": "minecraft:stone"}, "layers": [["#"]]}"####.to_string();
        let files: Vec<(String, String)> = (0..=MAX_PER_MOD)
            .map(|i| {
                (
                    format!("{}:s{i}", if i % 2 == 0 { "m" } else { "n" }),
                    one.clone(),
                )
            })
            .collect();
        let table = build(&files);
        assert_eq!(table.of_mod("m").len(), MAX_PER_MOD / 2 + 1);
        assert_eq!(table.of_mod("n").len(), MAX_PER_MOD / 2);
        let files: Vec<(String, String)> = (0..=MAX_PER_MOD)
            .map(|i| (format!("m:s{i}"), one.clone()))
            .collect();
        assert_eq!(build(&files).of_mod("m").len(), MAX_PER_MOD);
        // A later file with the same id wins.
        let files = [
            ("m:s".to_string(), one.clone()),
            (
                "m:s".to_string(),
                r####"{"palette": {"#": "minecraft:dirt"}, "layers": [["#"]]}"####.to_string(),
            ),
        ];
        assert_eq!(
            build(&files).of_mod("m")[0].cells[0].blocks,
            ["minecraft:dirt".to_string()]
        );
    }

    #[test]
    fn checks_whole_and_mismatched_shapes() {
        let s = core();
        // The core's cells around its anchor at (0, 63, 0): absolute places.
        let iron: Vec<(i32, i32, i32, &str)> = s
            .cells
            .iter()
            .filter(|c| c.blocks == ["minecraft:iron_block".to_string()])
            .map(|c| (c.dx, 63 + c.dy, c.dz, "minecraft:iron_block"))
            .collect();
        assert_eq!(iron.len(), 24);
        let mut built = iron.clone();
        built.push((0, 63, 0, "minecraft:lodestone"));
        let whole = |of: &[(i32, i32, i32, &str)]| check_cells(&s, [0, 63, 0], 0, &world(of));
        assert_eq!(whole(&built), Ok(None));
        // The first missing iron, by the cells' order: the smallest offset.
        let missing: Vec<_> = built
            .iter()
            .filter(|&&(x, y, z, _)| (x, y, z) != (-1, 62, -1))
            .copied()
            .collect();
        assert_eq!(
            whole(&missing),
            Ok(Some(vec![Mismatch {
                x: -1,
                y: 62,
                z: -1,
                expected: "minecraft:iron_block".into(),
                found: "minecraft:air".into(),
            }]))
        );
        // A state's properties are not matched; the found state is named.
        let turned: Vec<_> = built
            .iter()
            .map(|&(x, y, z, _)| {
                let state = if (x, y, z) == (0, 63, 0) {
                    "minecraft:lodestone"
                } else {
                    "minecraft:iron_block[axis=y]"
                };
                (x, y, z, state)
            })
            .collect();
        assert_eq!(whole(&turned), Ok(None));
        let caved: Vec<_> = built
            .iter()
            .map(|&(x, y, z, _)| (x, y, z, "minecraft:stone"))
            .collect();
        let mismatches = whole(&caved).unwrap().unwrap();
        assert_eq!(mismatches.len(), MISMATCHES);
        assert_eq!(mismatches[0].found, "minecraft:stone");
        // The anchor is a cell like any other: the lodestone must be there.
        let moved: Vec<_> = iron
            .iter()
            .chain([&(1, 63, 1, "minecraft:lodestone")])
            .copied()
            .collect();
        assert_eq!(whole(&moved).unwrap().unwrap().len(), 1);
        // A quarter past 3 is an error.
        assert_eq!(
            check_cells(&s, [0, 63, 0], 4, &world(&built)),
            Err("4 quarter turns: 0 to 3".into())
        );
    }

    #[test]
    fn checks_turn_the_quarters() {
        // A flat L, one layer: stone at (0, 0), (1, 0), (0, 1) and air at
        // (1, 1), x along the row and z down the rows (a space is air).
        let s = build(&[(
            "m:l".into(),
            r####"{"palette": {"#": "minecraft:stone"}, "layers": [["##", "# "]]}"####.into(),
        )])
        .of_mod("m")
        .first()
        .cloned()
        .unwrap();
        assert_eq!(s.cells.len(), 4);
        let at = [5, 64, 5];
        let shape = |offsets: &[(i32, i32)]| -> Vec<(i32, i32, i32, &str)> {
            offsets
                .iter()
                .map(|&(dx, dz)| (at[0] + dx, at[1], at[2] + dz, "minecraft:stone"))
                .collect()
        };
        // The shape's own place; the air cell matches the air around.
        let home = [(0, 0), (1, 0), (0, 1)];
        assert_eq!(check_cells(&s, at, 0, &world(&shape(&home))), Ok(None));
        // Quarter 1 turns (dx, dz) into (dz, -dx): the L faces east, and at
        // its own place one stone cell no longer fits.
        let east = [(0, 0), (0, -1), (1, 0)];
        assert_eq!(check_cells(&s, at, 1, &world(&shape(&east))), Ok(None));
        assert_eq!(
            check_cells(&s, at, 1, &world(&shape(&home)))
                .unwrap()
                .unwrap()
                .len(),
            1
        );
        // Quarters 2 and 3 are the same turn carried on.
        let south = [(0, 0), (-1, 0), (0, -1)];
        assert_eq!(check_cells(&s, at, 2, &world(&shape(&south))), Ok(None));
        let west = [(0, 0), (0, 1), (-1, 0)];
        assert_eq!(check_cells(&s, at, 3, &world(&shape(&west))), Ok(None));
        // A chunk not loaded stops the check.
        let unloaded = |_: i32, _: i32, _: i32| -> Option<String> { None };
        assert_eq!(
            check_cells(&s, at, 0, &unloaded),
            Err("a chunk there is not loaded".into())
        );
    }

    #[test]
    fn a_package_reads_only_its_own_structure_tree() {
        let dir = std::env::temp_dir().join(format!("horizon-multiblocks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let tree = dir.join("data/cells/multiblock");
        std::fs::create_dir_all(tree.join("tier/deep")).unwrap();
        std::fs::create_dir_all(dir.join("data/cells/other")).unwrap();
        std::fs::create_dir_all(dir.join("data/minecraft/multiblock")).unwrap();
        std::fs::write(tree.join("core.json"), CORE).unwrap();
        std::fs::write(tree.join("readme.txt"), "not a structure").unwrap();
        std::fs::write(tree.join("tier/deep/core.json"), CORE).unwrap();
        std::fs::write(dir.join("data/cells/other/x.json"), "{}").unwrap();
        std::fs::write(dir.join("data/minecraft/multiblock/x.json"), "{}").unwrap();
        // A folder mod: subfolders give ids, other trees are not read.
        let files = files_in_package(&dir, "cells");
        let ids: Vec<&str> = files.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["cells:core", "cells:tier/deep/core"]);
        // A `.hmod`: the same, from its entries; over-sized and non-UTF-8
        // files are skipped.
        let big = format!(
            r####"{{"palette": {{"#": "minecraft:stone"}}, "layers": [[{{"pad": "{}"}}]]}}"####,
            "x".repeat(MAX_FILE as usize + 64)
        );
        let zip_files = [
            ("data/cells/multiblock/core.json", CORE.as_bytes()),
            ("data/cells/multiblock/big.json", big.as_bytes()),
            ("data/cells/multiblock/bad.json", &[0xff, 0xfe, 0xfd]),
            ("data/minecraft/multiblock/x.json", b"{}"),
            ("assets/cells/multiblock/x.json", b"{}"),
            ("data/cells/multiblock/sub/dir/readme.md", b"{}"),
        ];
        let hmod = dir.join("cells.hmod");
        let manifest = "id = \"cells\"\nname = \"Cells\"\nversion = \"1\"\napi = \"0.3\"\n\
             side = \"server\"\ncapabilities = [\"block-entities\"]\n\
             experimental = [\"block-entities\"]\n";
        std::fs::write(
            &hmod,
            crate::manifest::Package::write_files(manifest, Some(b"\0asm"), &zip_files).unwrap(),
        )
        .unwrap();
        let files = files_in_package(&hmod, "cells");
        assert_eq!(files.len(), 1, "{files:?}");
        assert_eq!(files[0].0, "cells:core");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn watches_save_load_and_follow_their_limits() {
        let key = |x: i32| ("minecraft:overworld".to_string(), [x, 63, 0]);
        let mut watches = Watches::default();
        watches.watch("m", key(0), "m:core").unwrap();
        watches.watch("m", key(0), "m:tier2").unwrap();
        watches.watch("m", key(1), "m:core").unwrap();
        assert_eq!(watches.count("m"), 3);
        assert_eq!(watches.get("m", &key(0), "m:core"), Some(Watch::UNTOLD));
        // Retelling an anchored watch is not a new one; its state stays.
        watches.set(
            "m",
            &key(0),
            "m:core",
            Watch {
                quarter: 2,
                formed: true,
                told: false,
            },
        );
        watches.watch("m", key(0), "m:core").unwrap();
        assert_eq!(watches.count("m"), 3);
        assert_eq!(
            watches.get("m", &key(0), "m:core"),
            Some(Watch {
                quarter: 2,
                formed: true,
                told: false,
            })
        );
        let dir = std::env::temp_dir().join(format!("horizon-watches-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        watches.save(&dir).unwrap();
        let mut back = Watches::load(&dir);
        assert_eq!(
            back.get("m", &key(0), "m:core"),
            Some(Watch {
                quarter: 2,
                formed: true,
                told: false,
            })
        );
        assert_eq!(back.get("m", &key(0), "m:tier2"), Some(Watch::UNTOLD));
        assert_eq!(back.get("m", &key(1), "m:core"), Some(Watch::UNTOLD));
        // An anchor's watches go together, with its block entity.
        let gone = back.remove_anchor("m", &key(0));
        assert_eq!(
            gone.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(),
            ["m:core", "m:tier2"]
        );
        assert_eq!(back.count("m"), 1);
        // A mod may watch at most 256 structures; another mod counts
        // separately, and retelling is always fine.
        let mut full = Watches::default();
        for i in 0..MAX_WATCHES {
            full.watch("m", key(i as i32), "m:core").unwrap();
        }
        assert!(full.watch("m", key(MAX_WATCHES as i32), "m:core").is_err());
        assert_eq!(full.count("m"), MAX_WATCHES);
        full.watch("n", key(0), "n:core").unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
