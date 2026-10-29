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
//! `/cellcore` says it is whole or names the first block it needs. Its
//! power grids (capability `saved-data`) are data on the world itself:
//! `/cellnet <name>` joins the cell under the player to a grid, `/cellnet`
//! lists them; a grid goes when its last cell does, and a restart of the
//! mod loses nothing. Its energy staff (capability `item-hooks`) is a
//! blaze rod in the main hand: `/staff` reads its charge, a use spends it,
//! a blow with it deals twice over, a click or a tick on a cell moves
//! energy between the two, and a golden helmet worn takes a hurt a share
//! at a time — their charge is the mod's own bytes on the stack, saved
//! with it wherever it goes.
use horizon_mod_sdk::server::{
    Args, Argument, BlockEntity, Entity, HeldStack, ItemUsedBlock, MenuItem, MenuSlot, ModEntity,
    Multiblock, Player, PlayerStack, Registration, ServerMod, Shape, SlotMode, SpawnOptions, Stack,
    RedstoneOutput, Sides, TankSpec, Transform, UsedBlock, block_entities, displays, energy,
    entities, fluids, item_hooks, items, machine_recipes, menus, multiblocks, network, players,
    redstone, saved_data,
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

/// Capability `redstone`: a cell gives out a signal of its charge on every
/// face, a level for each [`GAUGE`] FE (a gauge for vanilla redstone), and
/// stops charging while a signal reaches its top face.
const GAUGE: u64 = 1000;

/// The signal a cell holding `stored` FE gives out.
fn gauge_level(stored: u64) -> u8 {
    (stored / GAUGE).min(15) as u8
}

/// A cell's tank (capability `fluids`): 16 buckets. A cell keeps half a
/// bucket and passes on what is above it, [`PIPE`] a tick, to the cells
/// beside it, so a pour spreads out and settles.
const TANK: u64 = 16_000;
const KEEP: u64 = 500;
const PIPE: u64 = 100;
/// What `/cellfluid` puts in or takes out.
const POUR: u64 = 1000;

/// A fluid as a player types it: `water` is `minecraft:water`.
fn fluid_name(word: &str) -> String {
    if word.contains(':') {
        word.to_owned()
    } else {
        format!("minecraft:{word}")
    }
}

/// What the cell at a place holds in its tank, as text.
fn tank_text(d: &str, x: i32, y: i32, z: i32) -> String {
    match fluids::get(d, x, y, z).and_then(|t| t.into_iter().next()) {
        Some(t) if t.amount > 0 => format!("{} {} / {} mB", t.fluid, t.amount, t.capacity),
        Some(t) => format!("empty / {} mB", t.capacity),
        None => "no tank".into(),
    }
}

/// The mob `/sentinel` spawns (capability `entities`): an iron golem the mod
/// calls `sentinel`. It takes a third of the damage a player's blow deals,
/// mends a point of health a second, answers a right click with its health
/// and tells the server when it falls.
const SENTINEL: &str = "sentinel";
const SENTINEL_KIND: &str = "minecraft:iron_golem";
/// Ticks between two points of health a sentinel mends.
const MEND_EVERY: u64 = 20;

/// The damage a sentinel lets a player's blow deal.
fn sentinel_damage(damage: f32) -> f32 {
    damage / 3.0
}

/// The cell each player's open menu shows, by player id.
#[derive(Default)]
struct Cells {
    open: BTreeMap<String, (String, i32, i32, i32)>,
    /// The cell each player's screen shows (capability `network`, the
    /// `cells` client mod's screen), by player id: what a player's messages
    /// act on, whatever position they name.
    screens: BTreeMap<String, (String, i32, i32, i32)>,
    /// The anchors whose core stands (capability `multiblocks`), by place.
    cores: std::collections::BTreeSet<(String, i32, i32, i32)>,
    /// The mod's grinding recipes, read once in `init` (capability
    /// `machine-recipes`): they never change while the mod runs.
    recipes: Vec<machine_recipes::MachineRecipe>,
    /// The mod's structures, read once in `init` (capability
    /// `multiblocks`): they never change while the mod runs either.
    structures: Vec<multiblocks::Structure>,
    /// The cells a signal reaches (capability `redstone`), told by
    /// `redstone_changed`: they don't charge.
    powered: std::collections::BTreeSet<(String, i32, i32, i32)>,
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

/// Turns `STEP` energy into a redstone dust in the cell's output slot, when
/// there is the energy and room: the energy left.
fn drain(d: &str, x: i32, y: i32, z: i32) -> Option<u64> {
    let now = stored(d, x, y, z);
    let made = count(d, x, y, z, OUTPUT);
    if now < STEP || made >= 64 {
        return None;
    }
    let dust = Stack {
        item: REDSTONE.into(),
        count: made + 1,
        components: String::new(),
    };
    items::set(d, x, y, z, OUTPUT, Some(&dust)).ok()?;
    let next = now - STEP;
    let _ = energy::set_stored(d, x, y, z, next);
    Some(next)
}

/// What the cell screen shows, as a message: `x y z stored capacity in out`.
fn screen_state(d: &str, x: i32, y: i32, z: i32) -> String {
    format!(
        "{x} {y} {z} {} {CAPACITY} {} {}",
        stored(d, x, y, z),
        count(d, x, y, z, INPUT),
        count(d, x, y, z, OUTPUT)
    )
}

/// The block under `player`, or `pos`.
fn target(player: &Player, args: &Args) -> (i32, i32, i32) {
    match args.block_pos("pos") {
        Some(p) => (p.x, p.y, p.z),
        None => under(player),
    }
}

/// The block under `player`.
fn under(player: &Player) -> (i32, i32, i32) {
    (
        player.x.floor() as i32,
        player.y.floor() as i32 - 1,
        player.z.floor() as i32,
    )
}

/// What a cell of the first version kept in its data (8 bytes,
/// little-endian): moved into its buffer when it gets one.
fn legacy_energy(data: &[u8]) -> u64 {
    data.try_into().map_or(0, u64::from_le_bytes)
}

/// The energy staff: a blaze rod held in the main hand (capability
/// `item-hooks`), its charge bytes of this mod's own on the stack, as
/// `legacy_energy`'s are on a cell.
const STAFF: &str = "minecraft:blaze_rod";
/// The helmet that shields (capability `item-hooks`): a golden helmet
/// worn, its charge on the stack as the staff's is.
const HELMET: &str = "minecraft:golden_helmet";
/// The most a staff or a helmet holds, and what flows either way between
/// one and a cell at a use or a tick on one.
const HELD_CAPACITY: u64 = 100_000;
/// What a use of the staff costs, and what a blow with it spends.
const USE_COST: u64 = 100;
const BLOW_COST: u64 = 10;
/// The most a hurt a charged helmet takes, an FE a point of it.
const HELMET_TAKE: f32 = 5.0;

/// The charge in a held stack's bytes of this mod's own.
fn held_energy(data: &[u8]) -> u64 {
    data.try_into().map_or(0, u64::from_le_bytes)
}

/// The bytes of a held stack's charge.
fn held_data(energy: u64) -> Vec<u8> {
    energy.to_le_bytes().to_vec()
}

/// What flows between a held stack holding `held` FE and a cell holding
/// `cell` FE: into the held stack (true) or the cell, and how much — up to
/// [`STEP`], never past either's capacity.
fn held_flow(held: u64, cell: u64) -> (bool, u64) {
    if held < cell {
        (true, STEP.min(cell - held).min(HELD_CAPACITY - held))
    } else {
        (false, STEP.min(held - cell).min(CAPACITY - cell))
    }
}

/// Moves energy between a held stack and the mod's cell at a place, both
/// ways, as the menu's Fill and Drain buttons do — the cell's own stored
/// written directly, for the cell is the mod's. What moved, and which way.
fn move_between(
    player: &Player,
    item: &str,
    held: u64,
    dimension: &str,
    x: i32,
    y: i32,
    z: i32,
    cell: u64,
) -> Option<(bool, u64)> {
    let (into_held, amount) = held_flow(held, cell);
    if amount == 0 {
        return None;
    }
    if into_held {
        if !set_held(player, item, held + amount) {
            return None;
        }
        let _ = energy::set_stored(dimension, x, y, z, cell - amount);
    } else {
        if !set_held(player, item, held - amount) {
            return None;
        }
        let _ = energy::set_stored(dimension, x, y, z, cell + amount);
    }
    Some((into_held, amount))
}

/// The slot of the player's one of `item` (capability `item-hooks`): the
/// hotbar for a staff, the armour slots for a helmet. A stack of more than
/// one is none — its charge would double on a split.
fn held_slot(player: &Player, item: &str) -> Option<u32> {
    let slots = match item {
        HELMET => 36..40,
        _ => 0..9,
    };
    slots.into_iter().find(|&slot| {
        item_hooks::stack(&player.id, slot).is_some_and(|held| {
            held.item == item && held.count == 1 && held_energy(&held.data) <= HELD_CAPACITY
        })
    })
}

/// Sets a held stack's charge (capability `item-hooks`): nothing to set
/// where the player holds no such stack.
fn set_held(player: &Player, item: &str, energy: u64) -> bool {
    match held_slot(player, item) {
        Some(slot) => item_hooks::set_data(&player.id, slot, Some(&held_data(energy))).is_ok(),
        None => false,
    }
}

/// What a use of the main hand's staff is.
#[derive(Debug, PartialEq, Eq)]
enum StaffUse {
    /// The charge left after a spend.
    Spend(u64),
    /// Not enough charge for one.
    Empty,
    /// A stack of more than one: charge is kept on the item, not a stack.
    Stack,
}

/// A use of the staff, from its count and charge.
fn staff_use(count: u32, energy: u64) -> StaffUse {
    if count > 1 {
        StaffUse::Stack
    } else if energy < USE_COST {
        StaffUse::Empty
    } else {
        StaffUse::Spend(energy - USE_COST)
    }
}

/// A blow with the staff: the charge left after its cost, where it has it
/// (and is one item); None keeps vanilla's damage.
fn staff_blow(count: u32, energy: u64) -> Option<u64> {
    if count == 1 && energy >= BLOW_COST {
        Some(energy - BLOW_COST)
    } else {
        None
    }
}

/// The hurt a charged helmet takes: the fixed share, or all its charge
/// left, an FE a point.
fn helmet_take(damage: f32, energy: u64) -> f32 {
    damage.min(HELMET_TAKE).min(energy as f32)
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

/// A grid name is a saved-data key: one path segment of `[a-z0-9_.-]{1,64}`,
/// not `.` or `..`.
fn grid_name_ok(name: &str) -> bool {
    let chars_ok = (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b));
    chars_ok && name != "." && name != ".."
}

/// One member of a grid, as the grid's value holds it, a line: where the
/// cell stands.
fn member(dimension: &str, x: i32, y: i32, z: i32) -> String {
    format!("{dimension} {x} {y} {z}")
}

/// The grid's value for its members: the lines joined. None for no members
/// — a grid is deleted, not kept empty.
fn grid_value(members: &[String]) -> Option<Vec<u8>> {
    (!members.is_empty()).then(|| members.join("\n").into_bytes())
}

/// The members a grid's value holds, parsed back.
fn members_of(value: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(value)
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The members without `member`: none where the grid empties, for a grid
/// does not outlive its last cell.
fn without(members: &[String], member: &str) -> Option<Vec<String>> {
    let mut left = members.to_vec();
    left.retain(|m| m != member);
    (!left.is_empty()).then_some(left)
}

/// The members of grid `name` (capability `saved-data`).
fn grid_members(name: &str) -> Vec<String> {
    saved_data::get(name).map_or_else(Vec::new, |value| members_of(&value))
}

/// The mod's grids with their cells (capability `saved-data`), in the
/// store's order. Read anew each call: nothing is kept in the mod's own
/// memory, so a restart of it loses nothing.
fn grids() -> Vec<(String, usize)> {
    saved_data::keys()
        .into_iter()
        .map(|name| {
            let cells = grid_members(&name).len();
            (name, cells)
        })
        .collect()
}

/// Joins the cell at a place to grid `name` (capability `saved-data`):
/// joining again changes nothing, the members a set.
fn join_grid(dimension: &str, x: i32, y: i32, z: i32, name: &str) -> Result<(), String> {
    let cell = member(dimension, x, y, z);
    let mut members = grid_members(name);
    if !members.contains(&cell) {
        members.push(cell);
    }
    let value = grid_value(&members).expect("one member at least");
    saved_data::set(name, &value)
}

/// Takes the cell at a place out of every grid it is in (capability
/// `saved-data`); a grid that empties goes with it, so the mod's data on
/// the world does not outlive its last cell.
fn leave_grids(dimension: &str, x: i32, y: i32, z: i32) {
    let cell = member(dimension, x, y, z);
    for name in saved_data::keys() {
        match without(&grid_members(&name), &cell) {
            Some(members) => {
                let value = grid_value(&members).expect("kept members");
                let _ = saved_data::set(&name, &value);
            }
            None => saved_data::delete(&name),
        }
    }
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
                .on_joins()
                .command_with("cell", [Argument::block_pos("pos").optional()])
                .command_with("cellgui", [Argument::block_pos("pos").optional()])
                .command_with("cellui", [Argument::block_pos("pos").optional()])
                .command_with("cellitems", [Argument::block_pos("pos").optional()])
                .command_with("cellcore", [Argument::block_pos("pos").optional()])
                .command_with("cellnet", [Argument::word("name").optional()])
                .command_with("cellrecipes", [])
                .command_with("sentinel", [])
                .command_with(
                    "cellfluid",
                    [
                        Argument::word("fluid").optional(),
                        Argument::block_pos("pos").optional(),
                    ],
                )
                .command_with("staff", []),
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
            // A tank (capability `fluids`) that shares with the cells beside
            // it, as the energy does.
            if fluids::get(d, x, y, z).is_none() {
                let _ = fluids::set_tanks(
                    d,
                    x,
                    y,
                    z,
                    &[TankSpec {
                        capacity: TANK,
                        max_receive: POUR,
                        max_extract: POUR,
                    }],
                );
            }
            if let Some(t) = fluids::get(d, x, y, z).and_then(|t| t.into_iter().next()) {
                let share = t.amount.saturating_sub(KEEP).min(PIPE);
                if share > 0 {
                    fluids::push(d, x, y, z, 0, share);
                }
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
                    if self.powered.contains(&(d.to_owned(), x, y, z)) {
                        0
                    } else if self.cores.contains(&(d.to_owned(), x, y, z)) {
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
                // The gauge: the host does nothing when it is the same.
                let _ = redstone::set_output(
                    dimension,
                    x,
                    y,
                    z,
                    RedstoneOutput::weak(gauge_level(e)),
                );
            }
        }
        if tick.is_multiple_of(REFRESH) {
            for (player, (d, x, y, z)) in &self.open {
                let _ = menus::set_items(player, "cell", &view(stored(d, *x, *y, *z)));
            }
            for (player, (d, x, y, z)) in &self.screens {
                let state = screen_state(d, *x, *y, *z);
                let _ = network::send_to(player, "cells/state", state.as_bytes());
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
        // The cell leaves its grids too; a grid that empties goes with it.
        leave_grids(&entity.dimension, entity.x, entity.y, entity.z);
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
        // A screen of the cell closes with it.
        let viewers: Vec<String> = self
            .screens
            .iter()
            .filter(|(_, at)| **at == here)
            .map(|(p, _)| p.clone())
            .collect();
        for p in viewers {
            self.screens.remove(&p);
            let _ = network::send_to(&p, "cells/gone", b"");
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
        if name == "cellfluid" {
            // Capability `fluids`: `read` (or no word) reads the tank, `drain` takes a
            // bucket out (and a bucket item, when the fluid has one, goes
            // to the chat), any fluid name puts a bucket in.
            let (x, y, z) = target(player, args);
            let d = &player.dimension;
            block_entities::get(d, x, y, z).ok_or("No cell there")?;
            if fluids::get(d, x, y, z).is_none() {
                return Err("The cell has no tank yet".into());
            }
            let word = args.string("fluid").unwrap_or("read");
            if word == "read" {
                return Ok(Some(format!("Cell at {x} {y} {z}: {}", tank_text(d, x, y, z))));
            }
            if word == "drain" {
                return match fluids::extract(d, x, y, z, 0, POUR, false) {
                    Some(out) => {
                        let bucket = fluids::bucket_of_fluid(&out.fluid);
                        Ok(Some(format!(
                            "Took {} mB of {}{}",
                            out.amount,
                            out.fluid,
                            bucket.map_or(String::new(), |b| format!(" ({b})"))
                        )))
                    }
                    None => Err("The tank is empty".into()),
                };
            }
            let fluid = fluid_name(word);
            let put = fluids::insert(d, x, y, z, &fluid, POUR, false);
            if put == 0 {
                return Err(format!("The tank takes no {fluid}"));
            }
            return Ok(Some(format!("Put {put} mB of {fluid} in")));
        }
        if name == "sentinel" {
            // Capability `entities`: a mob of the mod, two blocks from the
            // player, that takes right clicks and has a name over it.
            let id = entities::spawn(
                &player.dimension,
                player.x + 2.0,
                player.y,
                player.z,
                &SpawnOptions::new(SENTINEL_KIND, SENTINEL)
                    .takes_use()
                    .named("Sentinel"),
            )?;
            return Ok(Some(format!("A sentinel stands guard (entity {id})")));
        }
        if name == "staff" {
            // The staff in the main hand (capability `item-hooks`): a hook
            // hears the stack, not its slot, so this scans for it as the
            // hooks' own writes do.
            return Ok(Some(match held_slot(player, STAFF) {
                Some(slot) => match item_hooks::stack(&player.id, slot) {
                    Some(held) => format!(
                        "Staff in slot {slot}: {} / {HELD_CAPACITY} FE",
                        held_energy(&held.data)
                    ),
                    None => "No staff in hand: hold one blaze rod, alone".into(),
                },
                None => "No staff in hand: hold one blaze rod, alone".into(),
            }));
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
        if name == "cellnet" {
            // The cell under the player only: the parser has no
            // backtracking, so a `pos` argument before `name` would eat a
            // grid name as a coordinate.
            return match args.string("name") {
                // The grids with their cells, in the store's order.
                None => {
                    let grids = grids();
                    Ok(Some(if grids.is_empty() {
                        "No grids".into()
                    } else {
                        let named = |(name, cells): &(String, usize)| {
                            format!(
                                "{name} ({cells} cell{})",
                                if *cells == 1 { "" } else { "s" }
                            )
                        };
                        format!(
                            "Grids: {}",
                            grids.iter().map(named).collect::<Vec<_>>().join(", ")
                        )
                    }))
                }
                Some(grid) => {
                    if !grid_name_ok(grid) {
                        return Err("a grid name is [a-z0-9_.-]{1,64}".into());
                    }
                    let (x, y, z) = under(player);
                    let d = &player.dimension;
                    block_entities::get(d, x, y, z).ok_or("No cell there")?;
                    join_grid(d, x, y, z, grid)?;
                    Ok(Some(format!("Cell at {x} {y} {z} joined grid {grid}")))
                }
            };
        }
        let (x, y, z) = target(player, args);
        let d = &player.dimension;
        block_entities::get(d, x, y, z).ok_or("No cell there")?;
        if name == "cellgui" {
            self.open_menu(player, d, x, y, z)?;
            return Ok(None);
        }
        if name == "cellui" {
            // The real screen (capability `network`): the client half draws
            // it from this state, and answers with what the player presses.
            // A client without it hears the message and shows nothing.
            network::send_to(
                &player.id,
                "cells/open",
                screen_state(d, x, y, z).as_bytes(),
            )
            .map_err(|e| {
                format!(
                    "{e}: the cell screen needs the Cells client mod; /cellgui is the chest menu"
                )
            })?;
            self.screens.insert(player.id.clone(), (d.clone(), x, y, z));
            return Ok(Some("Opening the cell screen".into()));
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

    fn item_hooks_items() -> Vec<String> {
        vec![STAFF.into(), HELMET.into()]
    }

    fn item_used(&mut self, player: &Player, stack: &HeldStack) {
        if stack.item != STAFF {
            return;
        }
        match staff_use(stack.count, held_energy(&stack.data)) {
            StaffUse::Spend(left) => {
                if set_held(player, STAFF, left) {
                    players::tell(&player.id, &format!("The staff has {left} FE left"));
                }
            }
            StaffUse::Empty => {
                players::tell(
                    &player.id,
                    "The staff is empty: stand it on a cell to charge",
                );
            }
            StaffUse::Stack => {
                players::tell(
                    &player.id,
                    "Stack the staff one by one: its charge is kept on the item",
                );
            }
        }
    }

    fn item_used_on_block(&mut self, player: &Player, stack: &HeldStack, block: &ItemUsedBlock) {
        if stack.item != STAFF || block.state.split('[').next() != Some(CELL) || stack.count > 1 {
            return;
        }
        // Only one of the mod's cells: a buffer of its own.
        if block_entities::get(&block.dimension, block.x, block.y, block.z).is_none() {
            return;
        }
        let (x, y, z) = (block.x, block.y, block.z);
        let (held, cell) = (held_energy(&stack.data), stored(&block.dimension, x, y, z));
        match move_between(player, STAFF, held, &block.dimension, x, y, z, cell) {
            Some((true, amount)) => {
                players::tell(
                    &player.id,
                    &format!("{amount} FE into the staff ({} FE)", held + amount),
                );
            }
            Some((false, amount)) => {
                players::tell(
                    &player.id,
                    &format!("{amount} FE into the cell ({} FE)", cell + amount),
                );
            }
            None => {}
        }
    }

    fn inventory_tick(&mut self, _tick: u64, held: &[PlayerStack]) {
        for held in held {
            let stack = &held.stack;
            if stack.count > 1 {
                continue;
            }
            // A staff or a helmet standing on a cell charges from it, as a
            // use on one moves energy both ways.
            let energy = held_energy(&stack.data);
            if energy >= HELD_CAPACITY {
                continue;
            }
            let (x, y, z) = under(&held.player);
            let d = &held.player.dimension;
            if block_entities::get(d, x, y, z).is_none() {
                continue;
            }
            move_between(
                &held.player,
                &stack.item,
                energy,
                d,
                x,
                y,
                z,
                stored(d, x, y, z),
            );
        }
    }

    fn hurt_entity(
        &mut self,
        player: &Player,
        stack: &HeldStack,
        entity: &Entity,
        damage: f32,
    ) -> Result<f32, String> {
        if stack.item != STAFF {
            return Ok(damage);
        }
        let Some(left) = staff_blow(stack.count, held_energy(&stack.data)) else {
            return Ok(damage);
        };
        if !set_held(player, STAFF, left) {
            return Ok(damage);
        }
        players::tell(
            &player.id,
            &format!(
                "The staff doubles the blow on {} to {:.1}",
                entity.kind,
                damage * 2.0
            ),
        );
        Ok(damage * 2.0)
    }

    fn player_hurt(&mut self, player: &Player, damage: f32, source: &str) -> Result<f32, String> {
        let Some(slot) = held_slot(player, HELMET) else {
            return Ok(0.0);
        };
        let energy = match item_hooks::stack(&player.id, slot) {
            Some(held) => held_energy(&held.data),
            None => return Ok(0.0),
        };
        if energy < 1 {
            players::tell(&player.id, "The helmet is out of charge");
            return Ok(0.0);
        }
        let take = helmet_take(damage, energy);
        let _ = item_hooks::set_data(&player.id, slot, Some(&held_data(energy - take as u64)));
        players::tell(
            &player.id,
            &format!("The helmet takes {take:.1} of {damage:.1} {source}"),
        );
        Ok(take)
    }

    fn menu_clicked(&mut self, player: &Player, _menu: &str, slot: u32, _button: u8, _shift: bool) {
        let Some((d, x, y, z)) = self.open.get(&player.id).cloned() else {
            return;
        };
        let now = stored(&d, x, y, z);
        let next = match slot {
            DRAIN => match drain(&d, x, y, z) {
                Some(next) => next,
                None => return,
            },
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

    /// What the cell screen's buttons send (channel `cells/action`): `fill`,
    /// `drain` or `close`, acting on the cell the screen shows. Nothing the
    /// client names is trusted: the cell is the one the server opened for
    /// the player.
    fn client_message(&mut self, player: &Player, channel: &str, data: &[u8]) {
        if channel != "cells/action" {
            return;
        }
        let Some((d, x, y, z)) = self.screens.get(&player.id).cloned() else {
            return;
        };
        match data {
            b"close" => {
                self.screens.remove(&player.id);
                return;
            }
            b"fill" => {
                let now = stored(&d, x, y, z);
                let _ = energy::set_stored(&d, x, y, z, (now + STEP).min(CAPACITY));
            }
            b"drain" => {
                drain(&d, x, y, z);
            }
            _ => return,
        }
        let state = screen_state(&d, x, y, z);
        let _ = network::send_to(&player.id, "cells/state", state.as_bytes());
    }

    fn entity_tick(&mut self, tick: u64, mobs: &[ModEntity]) {
        if tick % MEND_EVERY != 0 {
            return;
        }
        for m in mobs {
            if m.tag == SENTINEL && !m.dying && m.health < m.max_health {
                let _ = entities::set_health(m.id, (m.health + 1.0).min(m.max_health));
            }
        }
    }

    fn entity_hurt(
        &mut self,
        _player: &Player,
        entity: &ModEntity,
        damage: f32,
    ) -> Result<f32, String> {
        Ok(if entity.tag == SENTINEL {
            sentinel_damage(damage)
        } else {
            damage
        })
    }

    fn entity_died(&mut self, entity: &ModEntity) {
        players::broadcast(&format!(
            "A sentinel fell at {:.0} {:.0} {:.0}",
            entity.x, entity.y, entity.z
        ));
    }

    fn redstone_changed(&mut self, dimension: &str, x: i32, y: i32, z: i32, input: &Sides) {
        let cell = (dimension.to_owned(), x, y, z);
        // Only the top face counts: the cells beside it give out their own
        // gauges, which would lock each other.
        if input.up > 0 {
            self.powered.insert(cell);
        } else {
            self.powered.remove(&cell);
        }
    }

    fn entity_used(&mut self, player: &Player, entity: &ModEntity) {
        players::tell(
            &player.id,
            &format!(
                "The sentinel salutes you: {} / {} health",
                entity.health, entity.max_health
            ),
        );
    }

    fn player_left(&mut self, player: &Player) {
        self.open.remove(&player.id);
        self.screens.remove(&player.id);
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
    fn the_gauge_rises_a_level_for_each_thousand() {
        assert_eq!(gauge_level(0), 0);
        assert_eq!(gauge_level(999), 0);
        assert_eq!(gauge_level(1000), 1);
        assert_eq!(gauge_level(15_000), 15);
        assert_eq!(gauge_level(100_000), 15);
    }

    #[test]
    fn fluids_are_named_the_way_players_type_them() {
        assert_eq!(fluid_name("water"), "minecraft:water");
        assert_eq!(fluid_name("cells:plasma"), "cells:plasma");
    }

    #[test]
    fn a_sentinel_takes_a_third_of_a_blow() {
        assert_eq!(sentinel_damage(6.0), 2.0);
        assert_eq!(sentinel_damage(0.0), 0.0);
    }

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

    #[test]
    fn grid_values_round_trip() {
        let members = [
            member("minecraft:overworld", 0, 63, 0),
            member("minecraft:overworld", 5, 63, 5),
        ];
        let value = grid_value(&members).expect("members");
        assert_eq!(members_of(&value), members);
        // The value is text a person can read in the world's file.
        assert_eq!(
            String::from_utf8(value).unwrap(),
            "minecraft:overworld 0 63 0\nminecraft:overworld 5 63 5"
        );
        assert_eq!(grid_value(&[]), None);
    }

    #[test]
    fn the_last_cell_takes_the_grid_with_it() {
        let cell = member("minecraft:overworld", 0, 63, 0);
        let alone = [cell.clone()];
        assert_eq!(without(&alone, &cell), None);
        let two = [cell, member("minecraft:overworld", 5, 63, 5)];
        let left = without(&two, &two[0]).expect("one left");
        assert_eq!(left, [two[1].clone()]);
        // A cell no grid holds changes nothing.
        assert_eq!(
            without(&two, &member("minecraft:overworld", 9, 9, 9)),
            Some(two.to_vec())
        );
    }

    #[test]
    fn grid_names_are_saved_data_keys() {
        assert_eq!(
            member("minecraft:overworld", 5, 63, 5),
            "minecraft:overworld 5 63 5"
        );
        for name in ["main", "aux", "n1", "a.b-c_d"] {
            assert!(grid_name_ok(name), "{name}");
        }
        for name in ["MAIN", "", "a b", "a/b", ".", "..", &"x".repeat(65)] {
            assert!(!grid_name_ok(name), "{name}");
        }
    }

    /// A held stack's charge, as the staff and the helmet keep it.
    #[test]
    fn held_charge_round_trips() {
        assert_eq!(held_energy(&[]), 0);
        assert_eq!(held_energy(&held_data(0)), 0);
        for energy in [1, USE_COST, 42_000, HELD_CAPACITY] {
            assert_eq!(held_energy(&held_data(energy)), energy);
        }
        // What is not a charge (not 8 bytes) reads as none, as the first
        // version's leftover data on a cell does.
        assert_eq!(held_energy(&[1, 2, 3]), 0);
    }

    /// What flows between a held stack and a cell: the emptier one fills,
    /// up to [`STEP`] and neither's capacity.
    #[test]
    fn held_flow_fills_the_emptier_side() {
        assert_eq!(held_flow(0, STEP), (true, STEP));
        assert_eq!(held_flow(500, STEP), (true, 500));
        // The held stack the fuller: the cell takes.
        assert_eq!(held_flow(9_000, 1_000), (false, STEP));
        // The held stack the fuller: the cell takes.
        assert_eq!(held_flow(STEP, 0), (false, STEP));
        assert_eq!(held_flow(2 * STEP, 1_500), (false, 500));
        // Neither past its capacity.
        assert_eq!(held_flow(HELD_CAPACITY - 10, CAPACITY), (true, 10));
        assert_eq!(held_flow(HELD_CAPACITY, CAPACITY - 10), (false, 10));
    }

    /// A use of the staff: a spend, an empty staff, or a stack — a stack
    /// of more than one carries no charge, a split copying a stack's
    /// components to both halves.
    #[test]
    fn a_use_of_the_staff_spends_or_refuses() {
        assert_eq!(staff_use(1, 500), StaffUse::Spend(400));
        assert_eq!(staff_use(1, USE_COST), StaffUse::Spend(0));
        assert_eq!(staff_use(1, USE_COST - 1), StaffUse::Empty);
        assert_eq!(staff_use(2, 5_000), StaffUse::Stack);
        assert_eq!(staff_use(64, 0), StaffUse::Stack);
    }

    /// A blow with the staff doubles while it has the cost; the helmet's
    /// take is the fixed share, or all its charge left, an FE a point.
    #[test]
    fn a_blow_and_a_hurt() {
        assert_eq!(staff_blow(1, BLOW_COST), Some(BLOW_COST - BLOW_COST));
        assert_eq!(staff_blow(1, BLOW_COST - 1), None);
        assert_eq!(staff_blow(2, 5_000), None);
        let take = |damage: f32, energy: u64| helmet_take(damage, energy);
        assert_eq!(take(10.0, 100), HELMET_TAKE);
        assert_eq!(take(3.5, 100), 3.5);
        assert_eq!(take(10.0, 3), 3.0);
        assert_eq!(take(10.0, 0), 0.0);
    }
}
