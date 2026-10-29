//! Capability `machine-recipes`' table (docs/mods.md, "Machine recipes"):
//! the recipes of the server mods' own machines, read once from each mod's
//! `data/<id>/recipe/` files — in its own package ([`files_in_package`]) and
//! in a content mod's carry of the same id ([`files_in_dir`]) — before the
//! mods start. Content, not state: nothing here is saved with the world; the
//! files are read again at every start.
use serde_json::Value;
use std::{collections::BTreeMap, io::Read, path::Path};

/// Machine recipes one mod may carry; the rest are skipped, said so in the
/// log.
pub const MAX_PER_MOD: usize = 512;
/// One recipe file's biggest size.
pub const MAX_FILE: u64 = 16 << 10;
/// Recipes `by-input` answers with, at most.
pub const BY_INPUT: usize = 64;

/// A stack as a machine recipe names it: an item and a count, no components
/// (vanilla's ingredients carry none; a machine's own fields live in its
/// `data`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stack {
    pub item: String,
    pub count: u32,
}

/// One recipe of a mod's machine, as its file says.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineRecipe {
    /// `<mod id>:<path>`: the file's place under `data/<id>/recipe/`.
    pub id: String,
    /// The file's `type`, as `cells:grinding`.
    pub kind: String,
    /// What goes in, slot by slot: each slot's list holds every stack that
    /// fits it (a tag's members); an empty list is a free slot.
    pub inputs: Vec<Vec<Stack>>,
    /// What comes out, as `inputs`.
    pub results: Vec<Vec<Stack>>,
    /// The file's whole JSON text, for the fields only the mod knows
    /// (energy, ticks, tiers).
    pub data: String,
}

/// Every server mod's machine recipes, by mod id: built once, before the
/// mods start, and given to [`crate::Mods::load_for`], so a mod's `init`
/// already reads its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MachineRecipes {
    by_mod: BTreeMap<String, Vec<MachineRecipe>>,
}

/// The `data/<id>/recipe/**.json` files under `root`, as
/// `(recipe id, JSON text)` with the id `<id>:<path>`: the content layer's
/// carry of a mod's recipes, as `horizon-forge convert` lays it down. A file
/// over [`MAX_FILE`], or one that is not UTF-8, is skipped with a line in
/// the log. Only the mod's own id is read: a server mod's manifest may list
/// no other namespace.
pub fn files_in_dir(root: &Path, id: &str) -> Vec<(String, String)> {
    let mut files = Vec::new();
    let tree = root.join("data").join(id).join("recipe");
    read_tree(&tree, &tree, &mut files);
    files
        .into_iter()
        .filter_map(|(name, text)| Some((recipe_id(id, &name)?, text)))
        .collect()
}

/// [`files_in_dir`] over the mod at `path`: a mod folder's
/// `data/<id>/recipe/` tree, or the same entries of its `.hmod`. A file
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
    let prefix = format!("data/{id}/recipe/");
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
            log::warn!("Horizon: machine recipe {name} does not read: skipped");
            continue;
        }
        if bytes.len() as u64 > MAX_FILE {
            log::warn!(
                "Horizon: machine recipe {name} is over {} KiB: skipped",
                MAX_FILE >> 10
            );
            continue;
        }
        match String::from_utf8(bytes) {
            Ok(text) => {
                if let Some(rid) = recipe_id(id, rest) {
                    out.push((rid, text));
                }
            }
            Err(_) => log::warn!("Horizon: machine recipe {name} is not UTF-8: skipped"),
        }
    }
    out
}

/// The `.json` files under `dir`, recursively, by their `/`-separated name
/// under `root` (a folder's subfolders are allowed: `recipe/grind/x.json`).
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
                    "Horizon: machine recipe {} is over {} KiB: skipped",
                    path.display(),
                    MAX_FILE >> 10
                );
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(text) => out.push((name, text)),
                Err(_) => log::warn!(
                    "Horizon: machine recipe {} does not read as text: skipped",
                    path.display()
                ),
            }
        }
    }
}

/// The recipe id of a file at `path` under `data/<id>/recipe/`: `<id>:<path>`
/// without the `.json`, or None for a name that is no plain resource
/// location (the same characters the content fence allows).
fn recipe_id(id: &str, path: &str) -> Option<String> {
    let plain = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-/".contains(&b))
    };
    let path = path.strip_suffix(".json")?;
    path.split('/').all(plain).then(|| format!("{id}:{path}"))
}

/// Parses recipe `files` — `(recipe id, JSON text)`, as
/// [`files_in_package`] and [`files_in_dir`] read them — into one table for
/// every mod: a recipe id names its mod before the `:`, and a later file
/// with the same id wins, so a mod's package beats a content mod's carry of
/// it. `tags` expands a `#tag` into its member items. A file that does not
/// parse, or whose slots are not the ingredient syntax, is skipped with a
/// line in the log: one bad file keeps neither the mod nor the other
/// recipes from loading.
pub fn build(files: &[(String, String)], tags: &dyn Fn(&str) -> Vec<String>) -> MachineRecipes {
    let mut parsed: BTreeMap<&str, MachineRecipe> = BTreeMap::new();
    for (id, text) in files {
        match parse(id, text, tags) {
            Some(recipe) => {
                parsed.insert(id, recipe);
            }
            None => continue,
        }
    }
    let mut by_mod: BTreeMap<String, Vec<MachineRecipe>> = BTreeMap::new();
    let mut over = Vec::new();
    for (_, recipe) in parsed {
        let m = recipe.id.split(':').next().unwrap_or_default().to_owned();
        let list = by_mod.entry(m.clone()).or_default();
        if list.len() < MAX_PER_MOD {
            list.push(recipe);
        } else if over.is_empty() || *over.last().expect("checked") != m {
            over.push(m);
        }
    }
    for m in over {
        log::warn!(
            "Horizon: mod {m} carries over {MAX_PER_MOD} machine recipes: the rest are skipped"
        );
    }
    MachineRecipes { by_mod }
}

/// One recipe file: None, with a line in the log, when it does not parse or
/// a slot is not the ingredient syntax.
fn parse(id: &str, text: &str, tags: &dyn Fn(&str) -> Vec<String>) -> Option<MachineRecipe> {
    let skipped = |why: String| {
        log::warn!("Horizon: machine recipe {id} skipped: {why}");
        None
    };
    let Ok(file) = serde_json::from_str::<Value>(text) else {
        return skipped("does not parse".into());
    };
    let Some(kind) = file.get("type").and_then(Value::as_str) else {
        return skipped("has no type".into());
    };
    // Both spellings of a one-slot machine occur in carried JSON.
    let inputs = match slots(&file, "inputs", &["input", "ingredient"], tags) {
        Ok(s) => s,
        Err(why) => return skipped(why),
    };
    let results = match slots(&file, "results", &["result", "output"], tags) {
        Ok(s) => s,
        Err(why) => return skipped(why),
    };
    Some(MachineRecipe {
        id: id.to_owned(),
        kind: kind.to_owned(),
        inputs,
        results,
        data: text.to_owned(),
    })
}

/// The slot lists of a file: `plural` holds a list of slots, a singular key
/// one slot; none of them, an empty machine (a generator with no item input
/// of its own).
fn slots(
    file: &Value,
    plural: &str,
    singular: &[&str],
    tags: &dyn Fn(&str) -> Vec<String>,
) -> Result<Vec<Vec<Stack>>, String> {
    if let Some(list) = file.get(plural) {
        let Some(list) = list.as_array() else {
            return Err(format!("{plural} is not a list"));
        };
        return list
            .iter()
            .map(|slot| one_slot(slot, tags).map_err(|e| format!("{plural}: {e}")))
            .collect();
    }
    for key in singular {
        if let Some(slot) = file.get(key) {
            return Ok(vec![
                one_slot(slot, tags).map_err(|e| format!("{key}: {e}"))?,
            ]);
        }
    }
    Ok(Vec::new())
}

/// One slot, as vanilla's ingredient syntax writes it: `"minecraft:redstone"`,
/// `"#c:ores/iron"` (the tag's members, expanded by `tags`), `["a", "#b"]`
/// (any of them), `{"id"|"item": "..", "count": n}` or `{"tag": "..",
/// "count": n}`. The stacks it holds; an empty list for `[]`, a free slot. A
/// form that names no item at all (a `#tag` this game does not know) is an
/// error, not a free slot: a machine whose input could never be held.
fn one_slot(slot: &Value, tags: &dyn Fn(&str) -> Vec<String>) -> Result<Vec<Stack>, String> {
    let stack = |item: &str, count: u32| {
        Ok(vec![Stack {
            item: item.to_owned(),
            count,
        }])
    };
    let count_of = |v: &Value| v.get("count").and_then(Value::as_u64).unwrap_or(1);
    match slot {
        Value::String(s) => match s.strip_prefix('#') {
            Some(tag) => tagged(tag, 1, tags),
            None => stack(s, 1),
        },
        // Any of: the options of each entry, in order.
        Value::Array(list) => {
            let mut out = Vec::new();
            for entry in list {
                out.append(&mut one_slot(entry, tags)?);
            }
            Ok(out)
        }
        Value::Object(o) => {
            let count = count_of(slot).min(u32::MAX as u64) as u32;
            for key in ["id", "item"] {
                if let Some(item) = o.get(key).and_then(Value::as_str) {
                    return stack(item, count);
                }
            }
            match o.get("tag").and_then(Value::as_str) {
                Some(tag) => tagged(tag, count, tags),
                None => Err("is not the ingredient syntax".into()),
            }
        }
        _ => Err("is not the ingredient syntax".into()),
    }
}

/// A `#tag`'s members as `count` each; the tag must name at least one item.
fn tagged(tag: &str, count: u32, tags: &dyn Fn(&str) -> Vec<String>) -> Result<Vec<Stack>, String> {
    let members = (tags)(tag);
    if members.is_empty() {
        return Err(format!("#{tag} names no item in this game"));
    }
    Ok(members
        .into_iter()
        .map(|item| Stack { item, count })
        .collect())
}

impl MachineRecipes {
    /// The recipes of mod `id` of these `kind`s (an empty list: every one),
    /// in their ids' order.
    pub fn of_mod(&self, id: &str, kinds: &[String]) -> Vec<MachineRecipe> {
        self.by_mod.get(id).map_or_else(Vec::new, |list| {
            list.iter()
                .filter(|r| kinds.is_empty() || kinds.contains(&r.kind))
                .cloned()
                .collect()
        })
    }

    /// Those of [`MachineRecipes::of_mod`] a slot of whose `inputs` takes
    /// `item` (a free slot takes anything): the recipes a machine holding it
    /// can make now, at most [`BY_INPUT`] of them.
    pub fn by_input(&self, id: &str, item: &str, kinds: &[String]) -> Vec<MachineRecipe> {
        let mut out = self.of_mod(id, kinds);
        out.retain(|r| {
            r.inputs
                .iter()
                .any(|slot| slot.is_empty() || slot.iter().any(|s| s.item == item))
        });
        out.truncate(BY_INPUT);
        out
    }

    /// Every mod with recipes (its id), in order.
    pub fn mods(&self) -> impl Iterator<Item = &str> {
        self.by_mod.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Package;

    /// A server mod's manifest that may carry machine recipes.
    const MANIFEST: &str = "id = \"cells\"\nname = \"Cells\"\nversion = \"1\"\napi = \"0.3\"\n\
         side = \"server\"\ncapabilities = [\"machine-recipes\"]\n\
         experimental = [\"machine-recipes\"]\n";

    fn build_one(id: &str, json: &str) -> MachineRecipes {
        build(&[(id.into(), json.into())], &|_| Vec::new())
    }

    #[test]
    fn reads_the_ingredient_forms() {
        let no_tags = &|_: &str| -> Vec<String> { Vec::new() };
        let tags = &|tag: &str| match tag {
            "c:ores" => vec!["minecraft:iron_ore".into(), "minecraft:copper_ore".into()],
            _ => Vec::new(),
        };
        let json = r##"{
            "type": "m:grinding",
            "inputs": ["minecraft:redstone", "#c:ores", ["minecraft:flint", "#c:ores"],
                       {"item": "minecraft:gunpowder", "count": 3}, {"tag": "c:ores", "count": 2}],
            "results": [{"id": "minecraft:iron_ingot", "count": 4}]
        }"##;
        let r = build(&[("m:r".into(), json.into())], tags)
            .of_mod("m", &[])
            .pop()
            .unwrap();
        assert_eq!(r.kind, "m:grinding");
        assert_eq!(r.id, "m:r");
        assert_eq!(r.data, json);
        let slot = |i: usize| r.inputs[i].clone();
        assert_eq!(
            slot(0),
            [Stack {
                item: "minecraft:redstone".into(),
                count: 1
            }]
        );
        assert_eq!(
            slot(1),
            [
                Stack {
                    item: "minecraft:iron_ore".into(),
                    count: 1
                },
                Stack {
                    item: "minecraft:copper_ore".into(),
                    count: 1
                },
            ]
        );
        // An array holds every option of its entries, in order.
        assert_eq!(slot(2).len(), 3);
        assert_eq!(slot(2)[2].item, "minecraft:copper_ore");
        assert_eq!(
            slot(3),
            [Stack {
                item: "minecraft:gunpowder".into(),
                count: 3
            }]
        );
        assert_eq!(
            slot(4)[0],
            Stack {
                item: "minecraft:iron_ore".into(),
                count: 2
            }
        );
        assert_eq!(
            r.results,
            [[Stack {
                item: "minecraft:iron_ingot".into(),
                count: 4
            }]]
        );
        // `[]` is a free slot; an unknown tag or another form is an error.
        assert_eq!(
            one_slot(&serde_json::json!([]), no_tags).unwrap(),
            Vec::<Stack>::new()
        );
        assert!(one_slot(&serde_json::json!("#c:nope"), tags).is_err());
        assert!(one_slot(&serde_json::json!(3), no_tags).is_err());
        assert!(one_slot(&serde_json::json!({"count": 2}), no_tags).is_err());
    }

    #[test]
    fn singular_keys_and_empty_machines() {
        let one = build_one(
            "m:gen",
            r#"{"type": "m:gen", "ingredient": "minecraft:lava", "output": "m:power"}"#,
        )
        .of_mod("m", &[])
        .pop()
        .unwrap();
        assert_eq!(one.inputs.len(), 1);
        assert_eq!(
            one.results,
            [[Stack {
                item: "m:power".into(),
                count: 1
            }]]
        );
        // No inputs, no results: the fields the mod alone knows carry it.
        let bare = build_one("m:bare", r#"{"type": "m:burn", "ticks": 200}"#)
            .of_mod("m", &[])
            .pop()
            .unwrap();
        assert!(bare.inputs.is_empty() && bare.results.is_empty());
        assert!(bare.data.contains("\"ticks\": 200"));
    }

    #[test]
    fn broken_files_are_skipped_not_fatal() {
        let files = [
            ("m:bad".to_string(), "{ not json".to_string()),
            ("m:notype".to_string(), r#"{"inputs": []}"#.to_string()),
            (
                "m:slot".to_string(),
                r#"{"type": "m:g", "inputs": [3]}"#.to_string(),
            ),
            (
                "m:tag".to_string(),
                r##"{"type": "m:g", "inputs": ["#c:none"]}"##.to_string(),
            ),
            (
                "m:good".to_string(),
                r#"{"type": "m:g", "inputs": ["minecraft:stone"]}"#.to_string(),
            ),
        ];
        let table = build(&files, &|_| Vec::new());
        assert_eq!(table.mods().collect::<Vec<_>>(), ["m"]);
        let kept = table.of_mod("m", &[]);
        let ids: Vec<&str> = kept.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["m:good"]);
        // An unknown result slot form skips the recipe too.
        assert_eq!(
            build_one("m:r", r#"{"type": "m:g", "results": [true]}"#)
                .of_mod("m", &[])
                .len(),
            0
        );
    }

    #[test]
    fn a_later_file_with_the_same_id_wins() {
        let files = [
            ("m:r".to_string(), r#"{"type": "m:carry"}"#.to_string()),
            ("m:r".to_string(), r#"{"type": "m:own"}"#.to_string()),
        ];
        assert_eq!(
            build(&files, &|_| Vec::new()).of_mod("m", &[])[0].kind,
            "m:own"
        );
    }

    #[test]
    fn kinds_filter_and_by_input_search() {
        let file = |id: &str, kind: &str, ins: &str, out: u32| {
            (
                format!("m:{id}"),
                format!(
                    r#"{{"type": "{kind}", "inputs": [{ins}], "results": [{{"id": "m:dust", "count": {out}}}]}}"#
                ),
            )
        };
        let files = [
            file("cobble", "m:grind", "\"minecraft:cobblestone\"", 2),
            file("andesite", "m:grind", "\"minecraft:andesite\"", 1),
            file("ore", "m:grind", "\"#c:ores\"", 1),
            file("free", "m:grind", "[]", 1),
            file("smelt", "m:smelt", "\"minecraft:cobblestone\"", 1),
        ];
        let expand = |tag: &str| match tag {
            "c:ores" => vec!["minecraft:cobblestone".to_string()],
            _ => Vec::new(),
        };
        let table = build(&files, &expand);
        assert_eq!(table.of_mod("m", &[]).len(), 5);
        let grind = table.of_mod("m", &["m:grind".to_string()]);
        assert_eq!(grind.len(), 4);
        // Cobblestone: itself, the ore tag's member and the free slot, in id
        // order; with every kind asked for, the smelter's recipe too.
        // Andesite takes only itself and the free slot.
        let cobblestone = table.by_input("m", "minecraft:cobblestone", &[]);
        let takes: Vec<&str> = cobblestone.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(takes, ["m:cobble", "m:free", "m:ore", "m:smelt"]);
        assert_eq!(table.by_input("m", "minecraft:andesite", &[]).len(), 2);
        assert_eq!(
            table
                .by_input("m", "minecraft:cobblestone", &["m:smelt".to_string()])
                .len(),
            1
        );
        // Another mod's recipes are not its.
        assert!(table.of_mod("other", &[]).is_empty());
        assert!(
            table
                .by_input("other", "minecraft:cobblestone", &[])
                .is_empty()
        );
    }

    #[test]
    fn by_input_answers_at_most_64() {
        let files: Vec<(String, String)> = (0..80)
            .map(|i| {
                (
                    format!("m:r{i}"),
                    r#"{"type": "m:g", "inputs": ["minecraft:stone"]}"#.to_string(),
                )
            })
            .collect();
        assert_eq!(
            build(&files, &|_| Vec::new())
                .by_input("m", "minecraft:stone", &[])
                .len(),
            BY_INPUT
        );
    }

    #[test]
    fn a_mod_carries_at_most_512() {
        let files: Vec<(String, String)> = (0..=MAX_PER_MOD)
            .map(|i| {
                (
                    format!("m:r{i}"),
                    r#"{"type": "m:g", "inputs": []}"#.to_string(),
                )
            })
            .collect();
        assert_eq!(
            build(&files, &|_| Vec::new()).of_mod("m", &[]).len(),
            MAX_PER_MOD
        );
        // Two mods count separately: 513 files alternating give m 257 and
        // n 256, both under the cap.
        let files: Vec<(String, String)> = (0..=MAX_PER_MOD)
            .map(|i| {
                (
                    format!("{}:r{i}", if i % 2 == 0 { "m" } else { "n" }),
                    r#"{"type": "m:g", "inputs": []}"#.to_string(),
                )
            })
            .collect();
        let table = build(&files, &|_| Vec::new());
        assert_eq!(table.of_mod("m", &[]).len(), 257);
        assert_eq!(table.of_mod("n", &[]).len(), 256);
    }

    #[test]
    fn a_package_reads_only_its_own_recipe_tree() {
        let dir = std::env::temp_dir().join(format!("horizon-recipes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let recipes = dir.join("data/cells/recipe");
        std::fs::create_dir_all(recipes.join("grind/deep")).unwrap();
        std::fs::create_dir_all(dir.join("data/cells/other")).unwrap();
        std::fs::create_dir_all(dir.join("data/minecraft/recipe")).unwrap();
        std::fs::write(
            recipes.join("grind/cobblestone.json"),
            r#"{"type": "cells:grinding"}"#,
        )
        .unwrap();
        std::fs::write(recipes.join("readme.txt"), "not a recipe").unwrap();
        std::fs::write(
            recipes.join("grind/deep/andesite.json"),
            r#"{"type": "cells:grinding"}"#,
        )
        .unwrap();
        std::fs::write(dir.join("data/cells/other/x.json"), "{}").unwrap();
        std::fs::write(dir.join("data/minecraft/recipe/x.json"), "{}").unwrap();
        // A folder mod: subfolders give ids, other trees are not read.
        let files = files_in_package(&dir, "cells");
        let ids: Vec<&str> = files.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            ids,
            ["cells:grind/cobblestone", "cells:grind/deep/andesite"]
        );
        // A `.hmod`: the same, from its entries; over-sized and non-UTF-8
        // files are skipped.
        let big = format!(
            "{}{}",
            r#"{"type": "cells:grinding", "pad": ""#,
            "x".repeat(MAX_FILE as usize + 64) + "\"}"
        );
        let zip_files = [
            (
                "data/cells/recipe/grind/cobblestone.json",
                r#"{"type": "cells:grinding"}"#.as_bytes(),
            ),
            ("data/cells/recipe/big.json", big.as_bytes()),
            ("data/cells/recipe/bad.json", &[0xff, 0xfe, 0xfd]),
            ("data/minecraft/recipe/x.json", b"{}"),
            ("assets/cells/recipe/x.json", b"{}"),
            ("data/cells/recipe/sub/dir/readme.md", b"{}"),
        ];
        let hmod = dir.join("cells.hmod");
        std::fs::write(
            &hmod,
            Package::write_files(MANIFEST, Some(b"\0asm"), &zip_files).unwrap(),
        )
        .unwrap();
        let files = files_in_package(&hmod, "cells");
        assert_eq!(files.len(), 1, "{files:?}");
        assert_eq!(files[0].0, "cells:grind/cobblestone");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
