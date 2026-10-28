//! Example Horizon server mod (`docs/mods.md`, "Block entities", "Energy")
//! with capabilities `block-entities` and `energy`: every lodestone a player
//! places becomes an energy cell, an energy buffer any mod's machine can
//! fill or drain. A cell charges itself each tick, shares its energy with the
//! cells beside it, and keeps it in the world's save; breaking one says what
//! was lost. `/cell [pos]` reads the cell under the player (or at `pos`);
//! `/cellgui [pos]`, or a right click on the cell, opens its menu
//! (capability `menus`): its charge, kept up to date, and buttons to drain
//! or fill it.
use horizon_mod_sdk::server::{
    Args, Argument, BlockEntity, MenuItem, Player, Registration, ServerMod, UsedBlock,
    block_entities, energy, menus, players,
};
use std::collections::BTreeMap;

/// The block that becomes a cell.
const CELL: &str = "minecraft:lodestone";
/// Energy a cell makes each tick, and the most it holds.
const CHARGE: u64 = 5;
const CAPACITY: u64 = 100_000;
/// Energy that may flow between two cells side by side each tick.
const FLOW: u64 = 100;

/// The cell each player's open menu shows, by player id.
#[derive(Default)]
struct Cells {
    open: BTreeMap<String, (String, i32, i32, i32)>,
}

/// The menu's buttons, by slot (3 rows of 9).
const DRAIN: u32 = 11;
const CHARGE_SLOT: u32 = 13;
const FILL: u32 = 15;
const CLOSE: u32 = 22;
/// Energy a button moves.
const STEP: u64 = 1000;
/// Ticks between two refreshes of an open menu.
const REFRESH: u64 = 10;

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
            &["Takes energy out of the cell"],
        ),
        MenuItem::new(CHARGE_SLOT, "minecraft:redstone_block", 1)
            .named(&format!("{stored} / {CAPACITY} FE"), &[&bar]),
        MenuItem::new(FILL, "minecraft:glowstone_dust", 1)
            .named(&format!("Fill {STEP} FE"), &["Puts energy into the cell"]),
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

/// One tick of every cell: each charges, then each pair side by side
/// moves energy from the fuller toward the emptier, up to [`FLOW`]. Takes
/// and gives back energies by position.
fn step(cells: &mut BTreeMap<(i32, i32, i32), u64>) {
    for e in cells.values_mut() {
        *e = (*e + CHARGE).min(CAPACITY);
    }
    let positions: Vec<_> = cells.keys().copied().collect();
    for &(x, y, z) in &positions {
        // Only the +x, +y and +z neighbours: each pair is seen once.
        for n in [(x + 1, y, z), (x, y + 1, z), (x, y, z + 1)] {
            let (Some(&a), Some(&b)) = (cells.get(&(x, y, z)), cells.get(&n)) else {
                continue;
            };
            let moved = (a.abs_diff(b) / 2).min(FLOW);
            let (from, to) = if a > b {
                ((x, y, z), n)
            } else {
                (n, (x, y, z))
            };
            *cells.get_mut(&from).expect("listed") -= moved;
            *cells.get_mut(&to).expect("listed") += moved;
        }
    }
}

impl ServerMod for Cells {
    fn init() -> Result<(Self, Registration), String> {
        Ok((
            Cells::default(),
            Registration::new()
                .command_with("cell", [Argument::block_pos("pos").optional()])
                .command_with("cellgui", [Argument::block_pos("pos").optional()]),
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
        let mut dimensions: BTreeMap<&str, BTreeMap<(i32, i32, i32), u64>> = BTreeMap::new();
        for e in entities {
            let (d, x, y, z) = (&e.dimension, e.x, e.y, e.z);
            // Other mods' machines may take up to FLOW a tick from a cell,
            // and give it as much.
            if energy::get(d, x, y, z).is_none() {
                let _ = energy::set_storage(d, x, y, z, CAPACITY, FLOW, FLOW);
                let _ = energy::set_stored(d, x, y, z, legacy_energy(&e.data));
                let _ = block_entities::set(d, x, y, z, &[]);
            }
            dimensions
                .entry(d)
                .or_default()
                .insert((x, y, z), stored(d, x, y, z));
        }
        for (dimension, mut cells) in dimensions {
            let before = cells.clone();
            step(&mut cells);
            for ((x, y, z), e) in cells {
                if before.get(&(x, y, z)) != Some(&e) {
                    let _ = energy::set_stored(dimension, x, y, z, e);
                }
            }
        }
        if tick.is_multiple_of(REFRESH) {
            for (player, (d, x, y, z)) in &self.open {
                let _ = menus::set_items(player, "cell", &view(stored(d, *x, *y, *z)));
            }
        }
    }

    fn block_entity_removed(&mut self, entity: &BlockEntity) {
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
        let (x, y, z) = target(player, args);
        let d = &player.dimension;
        block_entities::get(d, x, y, z).ok_or("No cell there")?;
        if name == "cellgui" {
            self.open_menu(player, d, x, y, z)?;
            return Ok(None);
        }
        Ok(Some(format!(
            "Cell at {x} {y} {z}: {} / {CAPACITY} FE",
            stored(d, x, y, z)
        )))
    }

    fn menu_clicked(&mut self, player: &Player, _menu: &str, slot: u32, _button: u8, _shift: bool) {
        let Some((d, x, y, z)) = self.open.get(&player.id).cloned() else {
            return;
        };
        let now = stored(&d, x, y, z);
        let next = match slot {
            DRAIN => now.saturating_sub(STEP),
            FILL => (now + STEP).min(CAPACITY),
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
        menus::open(&player.id, "cell", &title, 3, &view(stored(d, x, y, z)))?;
        self.open.insert(player.id.clone(), (d.to_owned(), x, y, z));
        Ok(())
    }
}

horizon_mod_sdk::export_server_mod!(Cells);

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_new_cell_is_empty_and_charges() {
        assert_eq!(legacy_energy(&[]), 0);
        assert_eq!(legacy_energy(&7u64.to_le_bytes()), 7);
        let mut cells = BTreeMap::from([((0, 64, 0), 0)]);
        step(&mut cells);
        assert_eq!(cells[&(0, 64, 0)], CHARGE);
        cells.insert((0, 64, 0), CAPACITY);
        step(&mut cells);
        assert_eq!(cells[&(0, 64, 0)], CAPACITY);
    }

    #[test]
    fn neighbours_share_and_others_do_not() {
        let mut cells = BTreeMap::from([((0, 64, 0), 1000), ((1, 64, 0), 0), ((5, 64, 0), 0)]);
        step(&mut cells);
        assert_eq!(cells[&(0, 64, 0)], 1005 - FLOW);
        assert_eq!(cells[&(1, 64, 0)], 5 + FLOW);
        assert_eq!(cells[&(5, 64, 0)], 5);
        let total: u64 = cells.values().sum();
        assert_eq!(total, 1015);
    }
}
