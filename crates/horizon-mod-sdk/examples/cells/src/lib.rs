//! Example Horizon server mod (`docs/mods.md`, "Block entities", "Energy")
//! with capabilities `block-entities` and `energy`: every lodestone a player
//! places becomes an energy cell, an energy buffer any mod's machine can
//! fill or drain. A cell charges itself each tick, shares its energy with the
//! cells beside it, and keeps it in the world's save; breaking one says what
//! was lost. `/cell [pos]` reads the cell under the player (or at `pos`);
//! `/cellgui [pos]`, or a right click on the cell, opens its menu
//! (capability `menus`): its charge, kept up to date, and buttons to drain
//! or fill it. Above each cell floats its charge, and a small redstone
//! core spins (capability `displays`). Each cell has two slots (capability
//! `items`), shown in its menu: redstone put in the left one turns into
//! energy, and the menu's Drain button turns energy into redstone in the
//! right one, which players can only take from. A cobblestone or andesite
//! in the left slot is ground into redstone by the mod's own machine
//! recipes (capability `machine-recipes`, `data/cells/recipe/`), which the
//! game loads before the mod starts; `/cellrecipes` counts them. Its
//! energy core is a multiblock (capability `multiblocks`,
//! `data/cells/multiblock/`): a ring of iron around the cell, watched from
//! its first tick, so the game tells when it comes together and when it no
//! longer holds; a standing core charges its cell ten times over, and
//! `/cellcore` says it is whole or names the first block it needs.
use horizon_mod_sdk::server::{
    Args, Argument, BlockEntity, MenuItem, MenuSlot, Multiblock, Player, Registration, ServerMod,
    Shape, SlotMode, Stack, Transform, UsedBlock, block_entities, displays, energy, items,
    machine_recipes, menus, multiblocks, players,
};
use std::collections::BTreeMap;

/// The block that becomes a cell.
const CELL: &str = "minecraft:lodestone";
/// Energy a cell makes each tick, and the most it holds. A standing core
/// charges its cell [`CORE_CHARGE`] times over.
const CHARGE: u64 = 5;
const CAPACITY: u64 = 100_000;
const CORE_CHARGE: u64 = 10;
/// Energy that may flow between two cells side by side each tick.
const FLOW: u64 = 100;

/// The cell each player's open menu shows, by player id.
#[derive(Default)]
struct Cells {
    open: BTreeMap<String, (String, i32, i32, i32)>,
    /// The anchors whose core stands (capability `multiblocks`), by place.
    cores: std::collections::BTreeSet<(String, i32, i32, i32)>,
    /// The mod's grinding recipes, read once in `init` (capability
    /// `machine-recipes`): they never change while the mod runs.
    recipes: Vec<machine_recipes::MachineRecipe>,
    /// The mod's structures, read once in `init` (capability
    /// `multiblocks`): they never change while the mod runs either.
    structures: Vec<multiblocks::Structure>,
}

/// The machine recipe kind this mod's files carry (`data/cells/recipe/`).
const KIND: &str = "cells:grinding";
/// The energy core's structure (`data/cells/multiblock/core.json`): a ring
/// of iron around the cell, hollow, with the cell itself as the anchor.
const CORE_ID: &str = "cells:core";

/// The menu's buttons, by slot (3 rows of 9).
const DRAIN: u32 = 11;
const CHARGE_SLOT: u32 = 13;
const FILL: u32 = 15;
const CLOSE: u32 = 22;
/// Puts a cobblestone in the input slot, to be ground (a demo button, like
/// Fill making energy).
const LOAD: u32 = 24;
/// Energy a button moves, and a redstone dust is worth.
const STEP: u64 = 1000;
/// The cell's slots: redstone in, redstone out; and where its menu shows them.
const INPUT: u32 = 0;
const OUTPUT: u32 = 1;
const INPUT_SLOT: u32 = 10;
const OUTPUT_SLOT: u32 = 16;
const REDSTONE: &str = "minecraft:redstone";
/// What the Load button puts in the input slot.
const COBBLE: &str = "minecraft:cobblestone";

/// How many items slot `slot` of the cell at a position holds.
fn count(d: &str, x: i32, y: i32, z: i32, slot: u32) -> u32 {
    items::get(d, x, y, z)
        .and_then(|s| s.get(slot as usize).cloned().flatten())
        .map_or(0, |s| s.count)
}

/// The stack a machine recipe makes first: its first result slot's first
/// option, as the cell's inventory holds it.
fn first_result(recipe: &machine_recipes::MachineRecipe) -> Option<Stack> {
    let made = recipe.results.first()?.first()?;
    Some(Stack {
        item: made.item.clone(),
        count: made.count,
        components: String::new(),
    })
}
/// Ticks between two refreshes of an open menu, and of the displays.
const REFRESH: u64 = 10;
/// The core's size, and how far it turns each refresh (the client turns it
/// smoothly in between).
const CORE: f32 = 0.3;
const TURN: f32 = 30.0;

/// The core's transform at `degrees`: scaled and turned about its own
/// centre (a display turns about its corner, so the turn is undone at the
/// centre by the translation).
fn core_transform(degrees: f32) -> Transform {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let c = CORE / 2.0;
    // Where the turn takes the centre (c, c, c), about the vertical axis.
    let (x, z) = (c * cos + c * sin, -c * sin + c * cos);
    Transform::identity()
        .scaled(CORE, CORE, CORE)
        .turned_y(degrees)
        .translated(-x, -c, -z)
}

/// The names of a cell's displays.
fn display_names(d: &str, x: i32, y: i32, z: i32) -> (String, String) {
    (
        format!("text {d} {x} {y} {z}"),
        format!("core {d} {x} {y} {z}"),
    )
}

/// The menu of a cell holding `stored`.
fn view(stored: u64) -> Vec<MenuItem> {
    let filled = (stored * 10 / CAPACITY) as usize;
    let bar = format!(
        "[{}{}] {}%",
        "|".repeat(filled),
        ".".repeat(10 - filled),
        stored * 100 / CAPACITY
    );
    vec![
        MenuItem::new(DRAIN, "minecraft:hopper", 1).named(
            &format!("Drain {STEP} FE"),
            &["Makes a redstone dust in the right slot"],
        ),
        MenuItem::new(CHARGE_SLOT, "minecraft:redstone_block", 1)
            .named(&format!("{stored} / {CAPACITY} FE"), &[&bar]),
        MenuItem::new(FILL, "minecraft:glowstone_dust", 1)
            .named(&format!("Fill {STEP} FE"), &["Puts energy into the cell"]),
        MenuItem::new(LOAD, COBBLE, 1).named(
            "Load cobble",
            &["Puts a cobblestone in the input slot, to be ground"],
        ),
        MenuItem::new(CLOSE, "minecraft:barrier", 1).named("Close", &[]),
    ]
}

/// The block under `player`, or `pos`.
fn target(player: &Player, args: &Args) -> (i32, i32, i32) {
    match args.block_pos("pos") {
        Some(p) => (p.x, p.y, p.z),
        None => (
            player.x.floor() as i32,
            player.y.floor() as i32 - 1,
            player.z.floor() as i32,
        ),
    }
}

/// What a cell of the first version kept in its data (8 bytes,
/// little-endian): moved into its buffer when it gets one.
fn legacy_energy(data: &[u8]) -> u64 {
    data.try_into().map_or(0, u64::from_le_bytes)
}

/// The energy in the buffer at a position; 0 where there is none yet.
fn stored(dimension: &str, x: i32, y: i32, z: i32) -> u64 {
    energy::get(dimension, x, y, z).map_or(0, |s| s.stored)
}

/// What the core around a cell is made of: whole (`Ok(None)`), or the
/// first block it needs and what is there instead, tried at every quarter
/// turn — a core built turned is whole too.
fn core_need(dimension: &str, x: i32, y: i32, z: i32) -> Result<Option<String>, String> {
    let mut first = None;
    for quarter in 0..4u32 {
        match multiblocks::check(dimension, x, y, z, quarter, CORE_ID) {
            Ok(None) => return Ok(None),
            Ok(Some(mismatches)) => first = first.or_else(|| mismatches.first().cloned()),
            Err(e) => return Err(e),
        }
    }
    Ok(first.map(|m| core_missing(&m)))
}

/// What one mismatch of the core says to the player.
fn core_missing(m: &multiblocks::Mismatch) -> String {
    format!(
        "needs {} at {} {} {} ({} there)",
        m.expected, m.x, m.y, m.z, m.found
    )
}

/// One tick of every cell: each charges by its gain (a standing core's
/// [`CORE_CHARGE`] times over), then each pair side by side moves energy
/// from the fuller toward the emptier, up to [`FLOW`]. Takes and gives
/// back energies by position.
fn step(cells: &mut BTreeMap<(i32, i32, i32), (u64, u64)>) {
    for e in cells.values_mut() {
        e.0 = (e.0 + e.1).min(CAPACITY);
    }
    let positions: Vec<_> = cells.keys().copied().collect();
    for &(x, y, z) in &positions {
        // Only the +x, +y and +z neighbours: each pair is seen once.
        for n in [(x + 1, y, z), (x, y + 1, z), (x, y, z + 1)] {
            let (Some(&a), Some(&b)) = (cells.get(&(x, y, z)), cells.get(&n)) else {
                continue;
            };
            let (a, b) = (a.0, b.0);
            let moved = (a.abs_diff(b) / 2).min(FLOW);
            let (from, to) = if a > b {
                ((x, y, z), n)
            } else {
                (n, (x, y, z))
            };
            cells.get_mut(&from).expect("listed").0 -= moved;
            cells.get_mut(&to).expect("listed").0 += moved;
        }
    }
}

impl ServerMod for Cells {
    fn init() -> Result<(Self, Registration), String> {
        // The game loads the mod's machine recipes before `init`
        // (docs/mods.md): read them once here and keep them, not each tick.
        let recipes = machine_recipes::all(&[KIND.to_string()]);
        let structures = multiblocks::all();
        Ok((
            Cells {
                recipes,
                structures,
                ..Cells::default()
            },
            Registration::new()
                .command_with("cell", [Argument::block_pos("pos").optional()])
                .command_with("cellgui", [Argument::block_pos("pos").optional()])
                .command_with("cellitems", [Argument::block_pos("pos").optional()])
                .command_with("cellcore", [Argument::block_pos("pos").optional()])
                .command_with("cellrecipes", []),
        ))
    }

    fn block_entity_blocks() -> Vec<String> {
        vec![CELL.into()]
    }

    fn block_uses() -> Vec<String> {
        vec![CELL.into()]
    }

    fn block_used(&mut self, player: &Player, b: &UsedBlock) {
        let _ = self.open_menu(player, &b.dimension, b.x, b.y, b.z);
    }

    fn block_entity_tick(&mut self, tick: u64, entities: &[BlockEntity]) {
        // Cells share only within a dimension.
        let mut dimensions: BTreeMap<&str, BTreeMap<(i32, i32, i32), (u64, u64)>> = BTreeMap::new();
        for e in entities {
            let (d, x, y, z) = (&e.dimension, e.x, e.y, e.z);
            // Other mods' machines may take up to FLOW a tick from a cell,
            // and give it as much.
            if energy::get(d, x, y, z).is_none() {
                let _ = energy::set_storage(d, x, y, z, CAPACITY, FLOW, FLOW);
                let _ = energy::set_stored(d, x, y, z, legacy_energy(&e.data));
                let _ = block_entities::set(d, x, y, z, &[]);
                // Ask after the core once, where the world is lent: the
                // answer says whether it already stands (events don't
                // repeat after a restart), and the game follows it from
                // here, telling formed and broken.
                if matches!(multiblocks::watch(d, x, y, z, CORE_ID), Ok(Some(_))) {
                    self.cores.insert((d.to_owned(), x, y, z));
                }
            }
            if items::get(d, x, y, z).is_none() {
                let _ = items::set_slots(d, x, y, z, 2);
            }
            // A machine recipe (capability `machine-recipes`) whose input
            // the cell holds: 1 item in, its first result out. The whole
            // result must fit, or nothing grinds. This takes the item before
            // the energy rule below can mistake it for redstone.
            if count(d, x, y, z, INPUT) > 0
                && let Some(held) = items::get(d, x, y, z)
                    .and_then(|slots| slots.get(INPUT as usize).cloned().flatten())
                && let Some(out) = machine_recipes::by_input(
                    &machine_recipes::Stack {
                        item: held.item,
                        count: 1,
                    },
                    &[KIND.to_string()],
                )
                .first()
                .and_then(first_result)
                && items::insert(d, x, y, z, &out, true) == out.count
            {
                let _ = items::extract(d, x, y, z, INPUT, 1, false);
                let _ = items::insert(d, x, y, z, &out, false);
            }
            // A redstone dust in the input slot: STEP more energy, if it fits.
            if count(d, x, y, z, INPUT) > 0 && stored(d, x, y, z) + STEP <= CAPACITY {
                let _ = items::extract(d, x, y, z, INPUT, 1, false);
                let _ = energy::set_stored(d, x, y, z, stored(d, x, y, z) + STEP);
            }
            dimensions.entry(d).or_default().insert(
                (x, y, z),
                (
                    stored(d, x, y, z),
                    if self.cores.contains(&(d.to_owned(), x, y, z)) {
                        CHARGE * CORE_CHARGE
                    } else {
                        CHARGE
                    },
                ),
            );
        }
        for (dimension, mut cells) in dimensions {
            let before = cells.clone();
            step(&mut cells);
            for ((x, y, z), (e, _)) in cells {
                if before.get(&(x, y, z)).map(|b| b.0) != Some(e) {
                    let _ = energy::set_stored(dimension, x, y, z, e);
                }
            }
        }
        if tick.is_multiple_of(REFRESH) {
            for (player, (d, x, y, z)) in &self.open {
                let _ = menus::set_items(player, "cell", &view(stored(d, *x, *y, *z)));
            }
            let degrees = (tick / REFRESH) as f32 * TURN % 360.0;
            for e in entities {
                let (d, x, y, z) = (&e.dimension, e.x, e.y, e.z);
                let (text, core) = display_names(d, x, y, z);
                let (cx, cz) = (f64::from(x) + 0.5, f64::from(z) + 0.5);
                let label = format!("{} FE", stored(d, x, y, z));
                let _ = displays::show(
                    &text,
                    d,
                    cx,
                    f64::from(y) + 1.6,
                    cz,
                    &Shape::Text(label),
                    Transform::identity(),
                    0,
                );
                let _ = displays::show(
                    &core,
                    d,
                    cx,
                    f64::from(y) + 1.2,
                    cz,
                    &Shape::Block("minecraft:redstone_block".into()),
                    core_transform(degrees),
                    REFRESH as u32,
                );
            }
        }
    }

    fn block_entity_removed(&mut self, entity: &BlockEntity) {
        let (text, core) = display_names(&entity.dimension, entity.x, entity.y, entity.z);
        displays::hide(&text);
        displays::hide(&core);
        let here = (entity.dimension.clone(), entity.x, entity.y, entity.z);
        let watching: Vec<String> = self
            .open
            .iter()
            .filter(|(_, at)| **at == here)
            .map(|(p, _)| p.clone())
            .collect();
        for p in watching {
            self.open.remove(&p);
            menus::close(&p);
        }
        let lost = stored(&entity.dimension, entity.x, entity.y, entity.z);
        if lost > 0 {
            players::broadcast(&format!(
                "The cell at {} {} {} is gone with {lost} FE",
                entity.x, entity.y, entity.z
            ));
        }
    }

    fn command(
        &mut self,
        player: &Player,
        name: &str,
        args: &Args,
    ) -> Result<Option<String>, String> {
        if name == "cellrecipes" {
            return Ok(Some(format!("{} grinding recipes", self.recipes.len())));
        }
        if name == "cellcore" {
            let (x, y, z) = target(player, args);
            let d = &player.dimension;
            block_entities::get(d, x, y, z).ok_or("No cell there")?;
            if !self.structures.iter().any(|s| s.id == CORE_ID) {
                return Err("The mod carries no core structure".into());
            }
            return match core_need(d, x, y, z)? {
                None => Ok(Some(format!("The core at {x} {y} {z} is whole"))),
                Some(need) => Ok(Some(need)),
            };
        }
        let (x, y, z) = target(player, args);
        let d = &player.dimension;
        block_entities::get(d, x, y, z).ok_or("No cell there")?;
        if name == "cellgui" {
            self.open_menu(player, d, x, y, z)?;
            return Ok(None);
        }
        if name == "cellitems" {
            let (i, o) = (count(d, x, y, z, INPUT), count(d, x, y, z, OUTPUT));
            return Ok(Some(format!("Cell at {x} {y} {z}: {i} in, {o} out")));
        }
        Ok(Some(format!(
            "Cell at {x} {y} {z}: {} / {CAPACITY} FE",
            stored(d, x, y, z)
        )))
    }

    fn multiblock_formed(&mut self, block: &Multiblock) {
        self.cores
            .insert((block.dimension.clone(), block.x, block.y, block.z));
        players::broadcast(&format!(
            "The core at {} {} {} formed",
            block.x, block.y, block.z
        ));
    }

    fn multiblock_broken(&mut self, block: &Multiblock) {
        self.cores
            .remove(&(block.dimension.clone(), block.x, block.y, block.z));
        players::broadcast(&format!(
            "The core at {} {} {} broke",
            block.x, block.y, block.z
        ));
    }

    fn menu_clicked(&mut self, player: &Player, _menu: &str, slot: u32, _button: u8, _shift: bool) {
        let Some((d, x, y, z)) = self.open.get(&player.id).cloned() else {
            return;
        };
        let now = stored(&d, x, y, z);
        let next = match slot {
            DRAIN => {
                let made = count(&d, x, y, z, OUTPUT);
                if now < STEP || made >= 64 {
                    return;
                }
                let dust = Stack {
                    item: REDSTONE.into(),
                    count: made + 1,
                    components: String::new(),
                };
                let _ = items::set(&d, x, y, z, OUTPUT, Some(&dust));
                now - STEP
            }
            FILL => (now + STEP).min(CAPACITY),
            LOAD => {
                // A demo button, like Fill making energy: a cobblestone in
                // the input slot, ground on the next tick.
                let cobble = Stack {
                    item: COBBLE.into(),
                    count: 1,
                    components: String::new(),
                };
                let _ = items::set(&d, x, y, z, INPUT, Some(&cobble));
                now
            }
            CLOSE => {
                self.open.remove(&player.id);
                menus::close(&player.id);
                return;
            }
            _ => return,
        };
        let _ = energy::set_stored(&d, x, y, z, next);
        let _ = menus::set_items(&player.id, "cell", &view(next));
    }

    fn menu_closed(&mut self, player: &Player, _menu: &str) {
        self.open.remove(&player.id);
    }
}

impl Cells {
    /// Opens the menu of the cell at a position for `player`.
    fn open_menu(
        &mut self,
        player: &Player,
        d: &str,
        x: i32,
        y: i32,
        z: i32,
    ) -> Result<(), String> {
        let title = format!("Energy Cell {x} {y} {z}");
        let slots = [
            MenuSlot {
                slot: INPUT_SLOT,
                inventory_slot: INPUT,
                mode: SlotMode::Input,
            },
            MenuSlot {
                slot: OUTPUT_SLOT,
                inventory_slot: OUTPUT,
                mode: SlotMode::Output,
            },
        ];
        let items = view(stored(d, x, y, z));
        menus::open_inventory(&player.id, "cell", &title, 3, &items, d, x, y, z, &slots)?;
        self.open.insert(player.id.clone(), (d.to_owned(), x, y, z));
        Ok(())
    }
}

horizon_mod_sdk::export_server_mod!(Cells);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_core_turns_about_its_centre() {
        for degrees in [0.0, 30.0, 90.0, 215.0] {
            let t = core_transform(degrees);
            let (qx, qy, qz, qw) = t.rotation;
            assert!((qx * qx + qy * qy + qz * qz + qw * qw - 1.0).abs() < 1e-5);
            // The centre, scaled then turned, then moved: back at the origin.
            let c = CORE / 2.0;
            let (sin, cos) = degrees.to_radians().sin_cos();
            let turned = (c * cos + c * sin, -c * sin + c * cos);
            let (tx, ty, tz) = t.translation;
            assert!((turned.0 + tx).abs() < 1e-5 && (c + ty).abs() < 1e-5);
            assert!((turned.1 + tz).abs() < 1e-5);
        }
    }

    #[test]
    fn the_menu_shows_the_charge() {
        let items = view(42_000);
        let charge = items.iter().find(|i| i.slot == CHARGE_SLOT).unwrap();
        assert!(
            charge.components.contains("42000 / 100000 FE"),
            "{}",
            charge.components
        );
        assert!(charge.components.contains("[||||......] 42%"));
        assert!(items.iter().all(|i| i.slot < 27));
    }

    /// The recipe files as `machine-recipes` loads them: cobblestone makes
    /// two redstone, andesite one, so the machine's choice by its input is
    /// told apart by the count.
    fn grinding(input: &str, count: u32) -> machine_recipes::MachineRecipe {
        machine_recipes::MachineRecipe {
            id: format!("cells:grind/{input}"),
            kind: KIND.into(),
            inputs: vec![vec![machine_recipes::Stack {
                item: format!("minecraft:{input}"),
                count: 1,
            }]],
            results: vec![vec![machine_recipes::Stack {
                item: REDSTONE.into(),
                count,
            }]],
            data: String::new(),
        }
    }

    #[test]
    fn grinding_makes_the_matching_recipes_result() {
        let recipes = [grinding("cobblestone", 2), grinding("andesite", 1)];
        // A machine holding an item makes the result of the recipe whose
        // input takes it — cobblestone two redstone, andesite one.
        let takes = |input: &str| {
            recipes
                .iter()
                .filter(|r| r.inputs.iter().flatten().any(|s| s.item == input))
                .map(first_result)
                .collect::<Vec<_>>()
        };
        let made = |input: &str| {
            takes(input)
                .first()
                .and_then(|s| s.as_ref().map(|s| (s.item.clone(), s.count)))
                .expect("a result")
        };
        assert_eq!(made("minecraft:cobblestone"), (REDSTONE.to_string(), 2));
        assert_eq!(made("minecraft:andesite"), (REDSTONE.to_string(), 1));
        // An item no recipe takes makes nothing; neither does a recipe with
        // no results.
        assert!(takes("minecraft:stone").is_empty());
        let bare = machine_recipes::MachineRecipe {
            results: Vec::new(),
            ..grinding("andesite", 1)
        };
        assert!(first_result(&bare).is_none());
    }

    #[test]
    fn a_new_cell_is_empty_and_charges() {
        assert_eq!(legacy_energy(&[]), 0);
        assert_eq!(legacy_energy(&7u64.to_le_bytes()), 7);
        let mut cells = BTreeMap::from([((0, 64, 0), (0, CHARGE))]);
        step(&mut cells);
        assert_eq!(cells[&(0, 64, 0)].0, CHARGE);
        // A standing core charges its cell ten times over.
        cells.insert((0, 64, 0), (0, CHARGE * CORE_CHARGE));
        step(&mut cells);
        assert_eq!(cells[&(0, 64, 0)].0, CHARGE * CORE_CHARGE);
        cells.insert((0, 64, 0), (CAPACITY, CHARGE));
        step(&mut cells);
        assert_eq!(cells[&(0, 64, 0)].0, CAPACITY);
    }

    #[test]
    fn neighbours_share_and_others_do_not() {
        let mut cells = BTreeMap::from([
            ((0, 64, 0), (1000, CHARGE)),
            ((1, 64, 0), (0, CHARGE)),
            ((5, 64, 0), (0, CHARGE)),
        ]);
        step(&mut cells);
        assert_eq!(cells[&(0, 64, 0)].0, 1005 - FLOW);
        assert_eq!(cells[&(1, 64, 0)].0, 5 + FLOW);
        assert_eq!(cells[&(5, 64, 0)].0, 5);
        let total: u64 = cells.values().map(|e| e.0).sum();
        assert_eq!(total, 1015);
    }

    #[test]
    fn cellcore_names_the_first_missing_block() {
        let missing = multiblocks::Mismatch {
            x: -1,
            y: 62,
            z: -1,
            expected: "minecraft:iron_block".into(),
            found: "minecraft:air[axis=y]".into(),
        };
        assert_eq!(
            core_missing(&missing),
            "needs minecraft:iron_block at -1 62 -1 (minecraft:air[axis=y] there)"
        );
    }
}
