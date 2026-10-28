//! Example Horizon server mod (`docs/mods.md`, "Block entities", "Energy")
//! with capabilities `block-entities` and `energy`: every lodestone a player
//! places becomes an energy cell, an energy buffer any mod's machine can
//! fill or drain. A cell charges itself each tick, shares its energy with the
//! cells beside it, and keeps it in the world's save; breaking one says what
//! was lost. `/cell [pos]` reads the cell under the player (or at `pos`).
use horizon_mod_sdk::server::{
    Args, Argument, BlockEntity, Player, Registration, ServerMod, block_entities, energy, players,
};
use std::collections::BTreeMap;

/// The block that becomes a cell.
const CELL: &str = "minecraft:lodestone";
/// Energy a cell makes each tick, and the most it holds.
const CHARGE: u64 = 5;
const CAPACITY: u64 = 100_000;
/// Energy that may flow between two cells side by side each tick.
const FLOW: u64 = 100;

struct Cells;

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
            Cells,
            Registration::new().command_with("cell", [Argument::block_pos("pos").optional()]),
        ))
    }

    fn block_entity_blocks() -> Vec<String> {
        vec![CELL.into()]
    }

    fn block_entity_tick(&mut self, _tick: u64, entities: &[BlockEntity]) {
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
    }

    fn block_entity_removed(&mut self, entity: &BlockEntity) {
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
        _name: &str,
        args: &Args,
    ) -> Result<Option<String>, String> {
        let (x, y, z) = match args.block_pos("pos") {
            Some(p) => (p.x, p.y, p.z),
            None => (
                player.x.floor() as i32,
                player.y.floor() as i32 - 1,
                player.z.floor() as i32,
            ),
        };
        let d = &player.dimension;
        block_entities::get(d, x, y, z).ok_or("No cell there")?;
        Ok(Some(format!(
            "Cell at {x} {y} {z}: {} / {CAPACITY} FE",
            stored(d, x, y, z)
        )))
    }
}

horizon_mod_sdk::export_server_mod!(Cells);

#[cfg(test)]
mod tests {
    use super::*;

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
