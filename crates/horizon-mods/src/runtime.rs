//! Loading and calling mods with wasmtime's component model.
use crate::manifest::{Manifest, Package, Side};
use server_bindings::{
    HorizonServerMod,
    exports::horizon::mods::server_events::Registration as ServerRegistration,
    horizon::mods::{players, world as mod_world},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};
use wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder, Trap,
    component::{Component, HasSelf, Linker},
};

mod bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "horizon-mod",
    });
}
/// The server mods' world: `log` and `storage` are the client world's.
mod server_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "horizon-server-mod",
        additional_derives: [PartialEq],
        with: {
            "horizon:mods/log": super::bindings::horizon::mods::log,
            "horizon:mods/storage": super::bindings::horizon::mods::storage,
        },
    });
}
/// Capability `ui`'s world beyond `horizon-mod` (docs/mods.md): a second
/// world, so mods built before it (no `ui-events` export) still instantiate
/// as `horizon-mod` and simply hear nothing of their widgets.
mod ui_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "ui-host",
    });
}
pub use bindings::horizon::mods::{
    game::{Clock, Hit, Player},
    hud::{Corner, Line},
};
use bindings::{
    HorizonMod,
    exports::horizon::mods::events::Registration,
    horizon::mods::{chat, command, game, hud, log as mod_log, storage},
};
pub use server_bindings::{
    exports::horizon::mods::server_events::{
        Argument, ArgumentKind, ArgumentValue, BlockPos, ServerCommand,
    },
    horizon::mods::players::{GameMode, Player as ServerPlayer},
    horizon::mods::world::BlockChange,
};
type ClientEvents = bindings::exports::horizon::mods::events::Guest;
type ServerEvents = server_bindings::exports::horizon::mods::server_events::Guest;
/// The widget model a `ui` screen or settings page shows (WIT `interface
/// ui`), for the game to draw with its menus' own widgets.
pub use ui_bindings::horizon::mods::ui::{
    ButtonWidget, ItemWidget, ListWidget, Screen as UiScreen, TextWidget, Widget as UiWidget,
};
use ui_bindings::{UiHost, horizon::mods::ui};
/// Capability `block-entities`' world beyond `horizon-server-mod`: a second
/// world, so server mods built before it (no `block-entity-events` export)
/// still instantiate and simply keep no block entities.
mod block_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "block-entity-host",
    });
}
pub use block_bindings::exports::horizon::mods::block_entity_events::BlockEntity;
use block_bindings::{
    BlockEntityHost,
    horizon::mods::{block_entities, energy, fluids, items as mod_items},
};
/// Capability `menus`' world: its own, so server mods built before it (no
/// `menu-events` export) still load. `players` is the server world's.
mod menu_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "menu-host",
        additional_derives: [PartialEq],
        with: {
            "horizon:mods/players": super::server_bindings::horizon::mods::players,
        },
    });
}
pub use menu_bindings::horizon::mods::menus::{MenuItem, MenuSlot, SlotMode};

/// The inventory a menu's bound slots show (`menus.open-inventory`).
#[derive(Clone, Debug, PartialEq)]
pub struct MenuInventory {
    pub dimension: String,
    pub pos: [i32; 3],
    pub slots: Vec<MenuSlot>,
}
/// `block-use-events`' world: its own, so server mods built before it
/// still load. `players` is the server world's.
mod use_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "use-host",
        with: {
            "horizon:mods/players": super::server_bindings::horizon::mods::players,
        },
    });
}
use menu_bindings::{MenuHost, horizon::mods::menus};
use use_bindings::{UseHost, exports::horizon::mods::block_use_events::UsedBlock};
/// Capability `entities`' world: its own, so server mods built before it (no
/// `entity-events` export) still load. `players` is the server world's.
mod entity_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "entity-host",
        additional_derives: [PartialEq],
        with: {
            "horizon:mods/players": super::server_bindings::horizon::mods::players,
        },
    });
}
/// Capability `config`'s world: its own, so server mods built before it (no
/// `config-events` export) still load.
mod config_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "config-host",
    });
}
use config_bindings::{ConfigHost, horizon::mods::config as mod_config};
/// How many ticks pass between two looks at the config files.
pub const CONFIG_POLL: u64 = 100;
/// Config files one hook call may write.
const CONFIG_WRITES_PER_CALL: u32 = 16;
/// Capability `game-events`' world: its own, so server mods built before it
/// (no `game-events` export) still load. `players` is the server world's.
mod game_event_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "game-events-host",
        with: {
            "horizon:mods/players": super::server_bindings::horizon::mods::players,
        },
    });
}
use game_event_bindings::GameEventsHost;
/// Capability `redstone`'s world: its own, so server mods built before it
/// (no `redstone-events` export) still load.
mod redstone_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "redstone-host",
    });
}
use redstone_bindings::{RedstoneHost, horizon::mods::redstone};
/// Block entities one `input-changed` pass tells a mod of.
const REDSTONE_PER_TICK: usize = 256;
/// A mob's options at spawn, as the mod gave them.
pub use entity_bindings::horizon::mods::entities::SpawnOptions as EntitySpawn;
use entity_bindings::{EntityHost, horizon::mods::entities};
/// Mobs one mod may have, and spawn in one call.
pub const MAX_ENTITIES: usize = 1024;
const SPAWNS_PER_CALL: u32 = 32;
/// Mobs one `tick` call is told of.
const ENTITIES_PER_TICK: usize = 512;

/// One mob of a mod, as the game tells it and the mod reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityInfo {
    pub id: u32,
    pub owner: String,
    pub kind: String,
    pub tag: String,
    pub dimension: String,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub health: f32,
    pub max_health: f32,
    pub dying: bool,
}
impl EntityInfo {
    fn to_wit(&self) -> entities::Entity {
        entities::Entity {
            id: self.id,
            kind: self.kind.clone(),
            tag: self.tag.clone(),
            dimension: self.dimension.clone(),
            x: self.pos[0],
            y: self.pos[1],
            z: self.pos[2],
            yaw: self.yaw,
            health: self.health,
            max_health: self.max_health,
            dying: self.dying,
        }
    }
}
/// The mobs lent to the mods for their calls, with what spawning needs: the
/// game lends one with [`Mods::lend_entities`] and takes it back after.
#[derive(Clone, Debug, Default)]
pub struct EntityState {
    /// The next entity id a spawn takes (the game sets its own after).
    pub next_id: u32,
    /// The dimension the server is simulating.
    pub dimension: String,
    /// Whether the game applies a spawn right after the call.
    pub can_spawn: bool,
    /// The mob kinds the game can spawn, `minecraft:zombie`.
    pub kinds: Arc<std::collections::HashSet<String>>,
    /// Every mob of every mod.
    pub entities: Vec<EntityInfo>,
}
/// The state lent for the current calls, shared by every mod's store.
type EntityStateSlot = Arc<Mutex<Option<EntityState>>>;
/// Capability `network`'s worlds, one for each side: their own, so mods built
/// before it (no `network-events` / `network-client-events` export) still
/// load. The server's takes `players` from the server world, and the
/// client's the interface `network` from the server's.
mod net_server_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "net-server-host",
        with: {
            "horizon:mods/players": super::server_bindings::horizon::mods::players,
        },
    });
}
mod net_client_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "net-client-host",
        with: {
            "horizon:mods/network": super::net_server_bindings::horizon::mods::network,
        },
    });
}
use net_client_bindings::NetClientHost;
use net_server_bindings::{NetServerHost, horizon::mods::network};
/// Messages one hook call may send, and their bytes.
const NET_MESSAGES_PER_CALL: u32 = 64;
const NET_BYTES_PER_CALL: usize = 256 * 1024;
/// Capability `item-hooks`' world: its own, so server mods built before it
/// (no `item-events` export) still load. `players` is the server world's.
mod item_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "item-host",
        with: {
            "horizon:mods/players": super::server_bindings::horizon::mods::players,
        },
    });
}
/// The struck entity as `hurt-entity` tells of it.
pub use item_bindings::exports::horizon::mods::item_events::Entity as ItemEntity;
/// One player's claimed stack in an `inventory-tick` list.
pub use item_bindings::exports::horizon::mods::item_events::PlayerStack;
/// The block under the crosshair in `used-on-block`.
pub use item_bindings::exports::horizon::mods::item_events::UsedBlock as ItemUsedBlock;
/// A stack as capability `item-hooks` hands it to a mod: the item, its count
/// and the mod's own bytes on it.
pub use item_bindings::horizon::mods::item_hooks::HeldStack;
use item_bindings::{ItemHost, horizon::mods::item_hooks};
/// Capability `displays`' imports.
mod display_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "display-host",
        additional_derives: [PartialEq],
    });
}
use display_bindings::horizon::mods::displays;
pub use display_bindings::horizon::mods::displays::{Shape as DisplayShape, Transform};
/// Capability `effects`' imports.
mod effects_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "effects-host",
    });
}
use effects_bindings::horizon::mods::effects;
/// Particles and sounds one hook call may send.
const EFFECTS_PER_CALL: u32 = 128;
/// Particles one `spawn-particle` sends.
const MAX_PARTICLES: u32 = 500;

/// A particle a mod sent, checked: the game finds the particle type by name.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectParticle {
    /// `minecraft:flame`.
    pub name: String,
    pub pos: [f64; 3],
    pub spread: [f32; 3],
    pub speed: f32,
    pub count: u32,
    pub color: Option<u32>,
    pub scale: Option<f32>,
    pub block: Option<String>,
    pub always_show: bool,
}
/// Displays one mod may show.
pub const MAX_DISPLAYS: usize = 1024;
/// How often a watched structure is checked again (capability
/// `multiblocks`): every 20 ticks, about a second.
const CHECK_EVERY: u64 = 20;
/// Block reads one check pass may spend, across every watch of a mod.
const CHECK_READS: usize = 8192;
/// Rows a menu may have, and so its slots.
const MENU_ROWS: u32 = 6;
/// Claimed blocks a mod may list (`block-entity-events.blocks`).
const CLAIMED_BLOCKS: usize = 256;
/// Items a mod may claim (`item-events.items`).
const CLAIMED_ITEMS: usize = 256;
/// Bytes of a mod's own on one stack (`item-hooks.set-data`): a component
/// on the stack, saved with it.
const STACK_DATA: usize = 4096;
/// `set-data` calls one hook call may make: each rewrites the stack's
/// components, so a loop of them needs its own bound beside fuel.
const SET_DATA_PER_CALL: u32 = 32;
/// Stacks one `inventory-tick` call gets; a longer list comes in turns
/// (the game walks it round-robin).
pub const TICK_ITEMS: usize = 512;
/// Capability `render`'s imports (docs/mods.md): the game draws the shapes.
mod render_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "render-host",
    });
}
use render_bindings::horizon::mods::input;
/// Capability `input`'s records (WIT `interface input`).
pub use render_bindings::horizon::mods::input::{Event as InputEvent, HeldItem, ItemUse};
/// What capability `render` asks the game to draw (WIT `interface render`).
pub use render_bindings::horizon::mods::render::{
    Corner as RenderCorner, FarTerrain as ModFarTerrain, FillShape, FrameRate, ItemShape,
    Minimap as ModMinimap, Point as RenderPoint, QuadShape, Rect as RenderRect, Shape, SpriteShape,
    TextShape, Zoom as ModZoom,
};
use render_bindings::{RenderHost, horizon::mods::render};
/// Capability `recipes`'s imports (docs/mods.md): the game's recipes and items.
mod recipes_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "recipes-host",
    });
}
/// Capability `ui`'s recipe viewer panel (WIT `interface browser`).
pub use recipes_bindings::horizon::mods::browser::{
    Click as BrowserClick, Event as BrowserEvent, ItemKey as BrowserItemKey,
    Layout as BrowserLayout, RecipeView as BrowserRecipe, SlotClick as BrowserSlotClick,
    Stack as BrowserStack,
};
/// What capability `recipes` reads (WIT `interface recipes`).
pub use recipes_bindings::horizon::mods::recipes::{
    Item as RecipeItem, Kind as RecipeKind, Recipe, Stack as RecipeStack,
};
use recipes_bindings::{RecipesHost, horizon::mods::browser, horizon::mods::recipes};
/// Capability `machine-recipes`' imports (docs/mods.md): the recipes of the
/// mods' own machines, a table built before they start.
mod machine_recipe_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "machine-recipes-host",
        additional_derives: [PartialEq],
    });
}
use machine_recipe_bindings::horizon::mods::machine_recipes;
/// Capability `multiblocks`' imports (docs/mods.md): the mods' own
/// structure templates, a table built before they start.
mod multiblock_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "multiblock-host",
        additional_derives: [PartialEq],
    });
}
pub use multiblock_bindings::exports::horizon::mods::multiblock_events::Multiblock;
pub use multiblock_bindings::horizon::mods::multiblocks::{Cell, Mismatch, Structure};
use multiblock_bindings::{MultiblockHost, horizon::mods::multiblocks};
/// Capability `saved-data`' imports (docs/mods.md): bytes each server mod
/// keeps on the world itself, saved with it.
mod saved_data_bindings {
    wasmtime::component::bindgen!({
        path: "../horizon-mod-sdk/wit",
        world: "saved-data-host",
    });
}
use saved_data_bindings::horizon::mods::saved_data;

/// A mod's linear memory cap.
const MEMORY: usize = 64 << 20;
/// Fuel (about one unit per WebAssembly instruction) per call: a bound on
/// runaway loops, far above the hooks' time budgets.
const FUEL_INIT: u64 = 2_000_000_000;
const FUEL_TICK: u64 = 50_000_000;
const FUEL_FRAME: u64 = 10_000_000;
const FUEL_EVENT: u64 = 100_000_000;
/// Blocks a mod may set per `world.set-blocks` call.
pub const BLOCKS_PER_CALL: usize = 4096;
/// Chat lines a mod may show per call, HUD lines and their length.
const CHAT_PER_CALL: usize = 8;
/// `command.run-quiet` lines one call of a mod may queue (a quiet payload's
/// cap, `horizon_protocol::quiet::MAX_LINES`).
const QUIET_PER_CALL: usize = 256;
/// How far `game.raycast` sees, in blocks.
pub const RAYCAST_MAX: f32 = 1024.0;
const HUD_LINES: usize = 16;
const LINE_CHARS: usize = 256;
/// Total bytes a mod's `storage` may hold on disk, across all its keys.
const STORAGE_QUOTA: u64 = 4 << 20;
/// Distinct keys a mod's `storage` may hold: `STORAGE_QUOTA` alone doesn't
/// bound a flood of zero- or near-zero-byte files (each still a file name,
/// a directory entry and an inode).
const STORAGE_MAX_KEYS: usize = 64;
/// Bytes `storage::set` may write across one hook call: fuel bounds a
/// mod's CPU time per call, not the time a synchronous disk write takes, so
/// a cheap-in-fuel loop replacing one key with large payloads needs its own
/// cap, separate from the on-disk quota it would otherwise stay under.
const STORAGE_WRITE_BUDGET: u64 = 1 << 20;

/// What mods may read of the game, copied once per tick by the game.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub player: Option<Player>,
    pub clock: Option<Clock>,
    /// The screen in GUI pixels (capability `render`'s `gui-size`).
    pub gui_size: (f32, f32),
    /// The main hand's stack (capability `input`'s `held`).
    pub held: Option<HeldItem>,
    /// The game's frame counter: fps, min, 1% low (`render.frame-stats`).
    pub frame: (u32, u32, u32),
    /// The movement and click mappings `input.mapping` answers for:
    /// (mapping, the bound key's label, held).
    pub mappings: Vec<(&'static str, String, bool)>,
    /// Attack and use clicks in the last second.
    pub cps: (u32, u32),
    /// The HUD font's advance per character from U+0000
    /// (`render.text-width`); empty: 6 each.
    pub font_widths: Arc<[f32]>,
    /// The players on the server, for server mods.
    pub players: Vec<ServerPlayer>,
    /// The first block along the player's view within [`RAYCAST_MAX`] and
    /// its distance from the eye, for `game.raycast`; the game works it out
    /// only while a mod calls it ([`Mods::wants_raycast`]).
    pub view_hit: Option<(f32, Hit)>,
    /// Every item and recipe, for capability `recipes`; the game makes them
    /// once, and only while a mod reads them ([`Mods::wants_recipes`]).
    pub recipes: Option<Arc<Recipes>>,
    /// The `book-id`s of the recipes the player's book holds.
    pub known_recipes: Arc<[i32]>,
    /// The open screen takes recipes (`recipes.place`).
    pub placing: bool,
    /// The player is in creative mode (`recipes.give`).
    pub creative: bool,
}

/// Capability `recipes`'s tables: `items` in the registry's order, which
/// each [`RecipeStack::item`] indexes.
#[derive(Clone, Debug, Default)]
pub struct Recipes {
    pub items: Vec<RecipeItem>,
    pub recipes: Vec<Recipe>,
}

/// Which mods a game starts: the window's game the client mods, the game
/// simulating the world the server mods, single player in the frame both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Runs {
    Client,
    Server,
    Both,
}
impl Runs {
    fn starts(self, side: Side) -> bool {
        match self {
            Runs::Both => true,
            Runs::Client => side != Side::Server,
            Runs::Server => side == Side::Server,
        }
    }
}

/// What mods asked the game to show since the last [`Mods::drain`].
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    /// `chat.show`: a line after the mod's name.
    Chat {
        name: String,
        text: String,
    },
    /// A mod command's feedback, red when `error`.
    Feedback {
        text: String,
        error: bool,
    },
    /// `command.run`: a vanilla command line, without its slash, to run as
    /// the player.
    Command {
        line: String,
    },
    /// `command.run-quiet`: vanilla command lines to run as the player with
    /// nothing in the chat (`horizon_protocol::quiet` on a server).
    QuietCommands {
        mod_id: String,
        lines: Vec<String>,
    },
    /// The mod was turned off: it trapped, overran or failed to start.
    Stopped {
        name: String,
        reason: String,
    },
    /// `players.tell`: a line after the mod's name, to the player with this id.
    Tell {
        name: String,
        player: String,
        text: String,
    },
    /// `players.broadcast`: a line after the mod's name, to every player.
    Broadcast {
        name: String,
        text: String,
    },
    /// A server mod command's feedback, to the player who typed it.
    Reply {
        player: String,
        text: String,
        error: bool,
    },
    /// `world.set-blocks`: changes to make in `dimension`, in order, each
    /// state checked by the [`WorldView`] lent for the call.
    SetBlocks {
        name: String,
        dimension: String,
        blocks: Vec<BlockChange>,
    },
    /// `ui.open-screen`: the mod's entry in `entries()`; what it shows is
    /// [`Mods::screen`].
    OpenScreen {
        owner: usize,
    },
    /// `ui.close-screen`: the mod closed its own screen (no `closed` back).
    CloseScreen {
        owner: usize,
    },
    /// `render.shake`: strength 0 to 1, fading out over `seconds` (≤ 10).
    Shake {
        strength: f32,
        seconds: f32,
    },
    /// `recipes.place`: the recipe book's click on this display id, all of
    /// it when `all`.
    PlaceRecipe {
        book_id: i32,
        all: bool,
    },
    /// `menus.open`: a chest menu of mod `mod_id` for the player with this
    /// id, its slots checked against its rows.
    OpenMenu {
        mod_id: String,
        player: String,
        menu: String,
        title: String,
        rows: u32,
        items: Vec<MenuItem>,
        /// `open-inventory`'s bound slots.
        inventory: Option<MenuInventory>,
    },
    /// Capability `items`: stacks to drop at a block (an inventory whose
    /// block went, slots an inventory lost).
    DropItems {
        dimension: String,
        pos: [i32; 3],
        stacks: Vec<crate::items::Stack>,
    },
    /// `menus.set-items`: the slots of that player's open menu `menu`.
    SetMenuItems {
        mod_id: String,
        player: String,
        menu: String,
        items: Vec<MenuItem>,
    },
    /// `menus.close`: that player's menu of mod `mod_id`, if open.
    CloseMenu {
        mod_id: String,
        player: String,
    },
    /// `displays.show`: mod `mod_id`'s display `name`, shown or changed.
    ShowDisplay {
        mod_id: String,
        name: String,
        dimension: String,
        pos: [f64; 3],
        shape: DisplayShape,
        transform: Transform,
        interpolation: u32,
    },
    /// `displays.hide`.
    HideDisplay {
        mod_id: String,
        name: String,
    },
    /// `network.send-to`: a message of server mod `mod_id` to the player
    /// with this id.
    NetToClient {
        mod_id: String,
        player: String,
        channel: String,
        data: Vec<u8>,
    },
    /// `network.send`: a message of client mod `mod_id` to its server half.
    NetToServer {
        mod_id: String,
        channel: String,
        data: Vec<u8>,
    },
    /// `entities.spawn`: a mob of mod `mod_id` with the entity id it was
    /// promised, at a position.
    SpawnEntity {
        mod_id: String,
        id: u32,
        pos: [f64; 3],
        options: EntitySpawn,
    },
    /// `entities.teleport`, `push` (a velocity), `set-health` and `remove`.
    TeleportEntity {
        mod_id: String,
        id: u32,
        pos: [f64; 3],
    },
    PushEntity {
        mod_id: String,
        id: u32,
        velocity: [f64; 3],
    },
    SetEntityHealth {
        mod_id: String,
        id: u32,
        health: f32,
    },
    RemoveEntity {
        mod_id: String,
        id: u32,
    },
    /// `redstone.set-output` (or the block entity's end): what the block at
    /// a position now gives out, in the order down, up, north, south, west,
    /// east; all zeros for nothing. The game updates the blocks around it.
    RedstoneOutput {
        dimension: String,
        pos: [i32; 3],
        power: [u8; 6],
        strong: bool,
    },
    /// `effects.spawn-particle`.
    Particle {
        dimension: String,
        particle: EffectParticle,
    },
    /// `effects.play-sound`: a sound at a position, on `SoundSource` index
    /// `category`.
    Sound {
        dimension: String,
        name: String,
        pos: [f64; 3],
        volume: f32,
        pitch: f32,
        category: i32,
    },
    /// `effects.play-sound-to`: a sound for one player, at their position.
    SoundTo {
        player: String,
        name: String,
        volume: f32,
        pitch: f32,
        category: i32,
    },
    /// `recipes.give`: a stack for a creative player.
    Give {
        item: String,
        count: u32,
        components: String,
    },
}

/// The game's world as capability `world` reads it during a call: the
/// game lends one with [`Mods::lend_world`] and takes it back after.
pub trait WorldView: std::any::Any + Send {
    /// The block state at a position, `minecraft:oak_log[axis=y]`; `None`
    /// where no chunk of that dimension is loaded.
    fn block(&self, dimension: &str, x: i32, y: i32, z: i32) -> Option<String>;
    /// The biome at a position, `minecraft:plains`; `None` as for blocks.
    fn biome(&self, dimension: &str, x: i32, y: i32, z: i32) -> Option<String>;
    /// Whether `dimension` is one the server has.
    fn has_dimension(&self, dimension: &str) -> bool;
    /// `Err` (why) for a state string that names no block state.
    fn check_state(&self, state: &str) -> Result<(), String>;
    /// An item's stack size; `None` for an item the game doesn't know.
    fn max_stack(&self, _item: &str) -> Option<u32> {
        None
    }
}

/// The view lent for the current calls, shared by every mod's store.
type WorldSlot = Arc<Mutex<Option<Box<dyn WorldView>>>>;
/// The server's players' inventories during a call: the game lends one
/// with [`Mods::lend_player_items`] and takes it back after. The stack in
/// `slot` of `player`'s inventory, as the mod's hooks number slots (36
/// slots, hotbar first, then armour feet to head 36..39, then the off hand
/// 40): the item's id, its count and the mod's own bytes on it.
pub trait PlayerItems: std::any::Any + Send {
    /// None for an empty slot, a player not on the server or a slot out of
    /// range.
    fn stack(&self, player: &str, slot: u32, mod_id: &str) -> Option<(String, u32, Vec<u8>)>;
    /// Replaces the mod's bytes on that stack (empty removes them): Err
    /// when the game cannot (no such player, no stack in the slot).
    fn set_data(
        &mut self,
        player: &str,
        slot: u32,
        mod_id: &str,
        data: &[u8],
    ) -> Result<(), String>;
}
/// Capability `block-entities`' store, lent like the world.
type EntitySlot = Arc<Mutex<Option<crate::BlockEntities>>>;
/// The players' inventories, lent like the world.
type PlayerSlot = Arc<Mutex<Option<Box<dyn PlayerItems>>>>;
/// Capability `machine-recipes`' table: static content, not lent world
/// state — filled once by [`Mods::load_for`] before any mod starts, and
/// every mod's `init` already reads it.
type RecipeSlot = Arc<Mutex<Option<Arc<crate::machine_recipes::MachineRecipes>>>>;
/// Capability `multiblocks`' table of structure templates, as
/// [`RecipeSlot`] is for recipes.
type TemplateSlot = Arc<Mutex<Option<Arc<crate::multiblocks::Templates>>>>;

/// A mod's text panel, drawn every frame until it changes.
#[derive(Clone, Debug)]
pub struct Panel {
    pub corner: Corner,
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Installed and turned off by the player.
    Off,
    Running,
    /// Turned on, and run by another game: a server mod in the window while
    /// the world runs on its server thread.
    Elsewhere,
    /// Could not load or start, or stopped while running.
    Failed(String),
}

/// One installed mod, for the Mods page.
pub struct Entry {
    pub path: PathBuf,
    /// None when the manifest could not be read (`status` says why).
    pub manifest: Option<Manifest>,
    pub status: Status,
    /// The mod's average time per call of its busiest hook.
    pub cost: Duration,
    run: Option<Running>,
    modified: Option<SystemTime>,
}
impl Entry {
    /// The running mod has a settings page (capability `ui` set widgets
    /// with `ui.set-settings`, or `input` registered keys the player may
    /// bind there): its row in Options > Mods gains a button.
    pub fn has_ui_settings(&self) -> bool {
        self.run.as_ref().is_some_and(|r| {
            let d = r.store.data();
            !d.ui.settings.is_empty() || !d.input.keys.is_empty()
        })
    }
}

struct Running {
    store: Store<State>,
    /// The mod's `ui-events` export; none for a mod built before `ui`, or a
    /// server mod.
    ui: Option<UiHost>,
    hooks: Hooks,
    /// It imports `game.raycast`.
    raycasts: bool,
    /// It imports capability `recipes`.
    reads_recipes: bool,
    /// It imports capability `machine-recipes`.
    reads_machine_recipes: bool,
    /// Capability `block-entities`: the export and the blocks it claims.
    blocks: Option<(BlockEntityHost, Vec<String>)>,
    /// Capability `menus`: the `menu-events` export.
    menus: Option<MenuHost>,
    /// `block-use-events` and the claimed blocks whose use the mod takes.
    uses: Option<(UseHost, Vec<String>)>,
    /// Capability `multiblocks`: the `multiblock-events` export.
    multiblocks: Option<MultiblockHost>,
    /// Capability `item-hooks`: the `item-events` export and the items it
    /// claims.
    items: Option<(ItemHost, Vec<String>)>,
    /// Capability `network`: the `network-events` export of a server mod, or
    /// the `network-client-events` of a client mod.
    net_server: Option<NetServerHost>,
    net_client: Option<NetClientHost>,
    /// Capability `entities`: the `entity-events` export.
    entities: Option<EntityHost>,
    /// Capability `redstone`: the `redstone-events` export.
    redstone: Option<RedstoneHost>,
    /// Capability `game-events`: the `game-events` export.
    game_events: Option<GameEventsHost>,
    /// Capability `config`: the `config-events` export.
    config: Option<ConfigHost>,
}

/// A running mod's exports and the hooks its `init` asked for.
enum Hooks {
    Client(HorizonMod, Registration),
    Server(HorizonServerMod, ServerRegistration),
}
impl Hooks {
    fn client(&self) -> Option<&Registration> {
        match self {
            Hooks::Client(_, r) => Some(r),
            Hooks::Server(..) => None,
        }
    }
    fn server(&self) -> Option<&ServerRegistration> {
        match self {
            Hooks::Server(_, r) => Some(r),
            Hooks::Client(..) => None,
        }
    }
    fn commands(&self) -> Vec<&str> {
        match self {
            Hooks::Client(_, r) => r.commands.iter().map(String::as_str).collect(),
            Hooks::Server(_, r) => r.commands.iter().map(|c| c.name.as_str()).collect(),
        }
    }
}

/// What capability `ui` pushed since the hooks last ran.
#[derive(Default)]
struct UiState {
    /// The screen `ui.open-screen` last pushed, until `ui.close-screen`.
    screen: Option<UiScreen>,
    /// The widgets `ui.set-settings` last pushed (the mod's page in
    /// Options > Mods).
    settings: Vec<UiWidget>,
    /// The calls since the last hook ended, for [`Output`].
    open: bool,
    close: bool,
}

/// Capability `ui`'s panel beside container screens (`browser`).
#[derive(Default)]
struct BrowserState {
    items: Vec<BrowserStack>,
    /// Which `set-items` or `set-recipes` call the panel is from, unique
    /// across mods and reloads, so the game converts them once.
    version: u64,
    /// `set-recipes`' title and recipes, shown instead of the items.
    recipes: Option<RecipePage>,
    events: std::collections::VecDeque<BrowserEvent>,
}
/// What `browser.set-recipes` shows: a title, tabs (`tab` the selected
/// one) and the recipes.
#[derive(Clone, Debug)]
pub struct RecipePage {
    pub title: String,
    pub tabs: Vec<BrowserStack>,
    pub tab: u32,
    pub recipes: Vec<BrowserRecipe>,
}
static BROWSER_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
/// Items a panel may hold, and events kept for `browser.poll`.
const BROWSER_ITEMS: usize = 8192;
/// `recipes.place` and `give` calls a hook call may make.
const RECIPE_ACTIONS: usize = 36;
/// Recipes `set-recipes` may show, slots in one, and stacks a slot cycles.
const BROWSER_RECIPES: usize = 256;
const RECIPE_SLOTS: usize = 9;
const RECIPE_TABS: usize = 16;
const SLOT_STACKS: usize = 64;
const BROWSER_EVENTS: usize = 256;
impl BrowserState {
    fn push(&mut self, event: BrowserEvent) {
        if self.events.len() == BROWSER_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}

struct State {
    name: String,
    id: String,
    snapshot: Arc<Snapshot>,
    out: Vec<Output>,
    panel: Option<Panel>,
    ui: UiState,
    /// Capability `render`'s HUD shapes, drawn until replaced.
    shapes: Vec<Shape>,
    /// Capability `render`'s corner panel.
    corner: Option<CornerPanel>,
    /// Capability `render`'s zoom, when the mod zooms.
    zoom: Option<ModZoom>,
    /// Capability `render`'s minimap, when the mod shows one.
    minimap: Option<ModMinimap>,
    /// Capability `render`'s far terrain, when the mod draws one.
    far_terrain: Option<ModFarTerrain>,
    input: InputState,
    browser: BrowserState,
    /// Capability `storage`'s folder; made on first `set`.
    data_dir: PathBuf,
    /// Bytes `storage::set` may still write before this hook call ends
    /// (reset per call in [`Mods::call`]): fuel bounds a mod's CPU time but
    /// not disk I/O time, so a loop of small-fuel, large-payload `set`s
    /// needs its own budget.
    storage_budget: u64,
    limits: StoreLimits,
    world: WorldSlot,
    entities: EntitySlot,
    machine_recipes: RecipeSlot,
    multiblocks: TemplateSlot,
    player_items: PlayerSlot,
    /// The items the mod claims (capability `item-hooks`), as `items()`
    /// answered: `stack` and `set-data` take no other item.
    item_claims: Vec<String>,
    /// `set-data` calls this hook call has made (reset per call in
    /// [`Mods::call`]).
    item_writes: u32,
    /// A server mod (`network.send-to`) or a client mod (`network.send`).
    server: bool,
    /// Capability `entities`' lent mobs, and the spawns this call made.
    mod_entities: EntityStateSlot,
    entity_spawns: u32,
    /// Particles and sounds this call has sent.
    effects_sent: u32,
    /// Capability `config`: the mod's entries and values, the file they are
    /// kept in, and the writes this call made.
    config: crate::config::Config,
    config_path: PathBuf,
    config_writes: u32,
    /// Messages and bytes this hook call has sent (reset per call).
    net_sent: (u32, usize),
}

/// The installed mods and the engine running them. `Mods::default()` is
/// the state with none: no engine, and every hook a no-op.
#[derive(Default)]
pub struct Mods {
    engine: Option<(Engine, Linker<State>)>,
    entries: Vec<Entry>,
    snapshot: Arc<Snapshot>,
    out: Vec<Output>,
    world: WorldSlot,
    entities: EntitySlot,
    machine_recipes: RecipeSlot,
    multiblocks: TemplateSlot,
    player_items: PlayerSlot,
    /// Capability `entities`' lent mobs.
    mod_entities: EntityStateSlot,
    /// Moves on whenever a mod starts or stops: the game resends the server
    /// mods' commands to players when it changes.
    generation: u64,
}

impl Mods {
    /// Changes whenever the running mods (so their commands) change.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Whether a running mod may use capability `world`, or keeps block
    /// entities (checked against the world each tick), or keeps its own
    /// data on the world (read and written through the lent stores): only
    /// then is a view worth lending.
    pub fn wants_world(&self) -> bool {
        self.entries.iter().any(|e| {
            e.status == Status::Running
                && (e.run.as_ref().is_some_and(|r| r.blocks.is_some())
                    || e.manifest.as_ref().is_some_and(|m| {
                        m.capabilities.contains(&crate::Capability::World)
                            || m.capabilities.contains(&crate::Capability::Multiblocks)
                            || m.capabilities.contains(&crate::Capability::SavedData)
                    }))
        })
    }
    /// `state` was placed at `pos`: each running mod claiming its block gets
    /// a new block entity there, in the lent store.
    pub fn block_entity_placed(&mut self, dimension: &str, pos: [i32; 3], state: &str) {
        let Ok(mut slot) = self.entities.lock() else {
            return;
        };
        if let Some(store) = slot.as_mut() {
            claim(store, &self.block_entity_claims(), dimension, pos, state);
        }
    }
    /// Lends the game's world to capability `world` for the calls until
    /// [`Mods::take_world`]; with none lent, `get-block` finds nothing and
    /// `set-blocks` checks only its count.
    pub fn lend_world(&mut self, view: Box<dyn WorldView>) {
        if let Ok(mut slot) = self.world.lock() {
            *slot = Some(view);
        }
    }
    /// The view [`Mods::lend_world`] lent, back to the game.
    pub fn take_world(&mut self) -> Option<Box<dyn WorldView>> {
        self.world.lock().ok()?.take()
    }
    /// Lends the world's block entities to capability `block-entities` for
    /// the calls until [`Mods::take_block_entities`].
    pub fn lend_block_entities(&mut self, store: crate::BlockEntities) {
        if let Ok(mut slot) = self.entities.lock() {
            *slot = Some(store);
        }
    }
    /// The store [`Mods::lend_block_entities`] lent, back to the game.
    pub fn take_block_entities(&mut self) -> Option<crate::BlockEntities> {
        self.entities.lock().ok()?.take()
    }
    /// Lends the server's players' inventories to capability `item-hooks`
    /// for the calls until [`Mods::take_player_items`]; with none lent,
    /// `stack` finds nothing and `set-data` is an error.
    pub fn lend_player_items(&mut self, view: Box<dyn PlayerItems>) {
        if let Ok(mut slot) = self.player_items.lock() {
            *slot = Some(view);
        }
    }
    /// The view [`Mods::lend_player_items`] lent, back to the game (its
    /// `set-data` writes are the game's to apply).
    pub fn take_player_items(&mut self) -> Option<Box<dyn PlayerItems>> {
        self.player_items.lock().ok()?.take()
    }
    /// Whether a running mod claims items (capability `item-hooks`): only
    /// then are the players' inventories worth lending.
    pub fn wants_player_items(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.run.as_ref().is_some_and(|r| r.items.is_some()))
    }
    /// Each running mod with block entities: its id and the blocks it claims.
    pub fn block_entity_claims(&self) -> Vec<(String, Vec<String>)> {
        self.entries
            .iter()
            .filter_map(|e| {
                let run = e.run.as_ref()?;
                let (_, blocks) = run.blocks.as_ref()?;
                Some((run.store.data().id.clone(), blocks.clone()))
            })
            .collect()
    }
    /// After the server mods' `tick`: drops the block entities whose block
    /// is gone (calling `removed`), then gives each mod its block entities
    /// in loaded chunks. Needs the world and the store lent.
    pub fn block_entity_tick(&mut self, tick: u64) {
        for i in 0..self.entries.len() {
            let Some(run) = self.entries[i].run.as_ref() else {
                continue;
            };
            let Some((_, claimed)) = run.blocks.as_ref() else {
                continue;
            };
            let id = run.store.data().id.clone();
            let Some((live, gone)) = self.sort_block_entities(&id, claimed) else {
                return;
            };
            for e in gone {
                self.call(i, FUEL_EVENT, |run| match &run.blocks {
                    Some((h, _)) => h
                        .horizon_mods_block_entity_events()
                        .call_removed(&mut run.store, &e),
                    None => Ok(()),
                });
                // Its buffer and inventory stay readable in `removed`, then
                // go: the inventory's items drop where the block was, and
                // the watches on it go with it — a structure that was whole
                // around it is told broken, with its last data.
                let mut broke = Vec::new();
                if let Ok(mut slot) = self.entities.lock()
                    && let Some(store) = slot.as_mut()
                {
                    let key = (e.dimension.clone(), [e.x, e.y, e.z]);
                    for (structure, w) in store.multiblocks.remove_anchor(&id, &key) {
                        if w.formed {
                            broke.push(Multiblock {
                                dimension: e.dimension.clone(),
                                x: e.x,
                                y: e.y,
                                z: e.z,
                                state: e.state.clone(),
                                data: e.data.clone(),
                                structure,
                                quarter: w.quarter,
                            });
                        }
                    }
                    store.energy.remove(&id, &key);
                    store.fluids.remove(&id, &key);
                    if store.redstone.remove(&id, &key) {
                        self.out.push(Output::RedstoneOutput {
                            dimension: key.0.clone(),
                            pos: key.1,
                            power: [0; 6],
                            strong: false,
                        });
                    }
                    let stacks = store.items.remove(&id, &key);
                    if !stacks.is_empty() {
                        self.out.push(Output::DropItems {
                            dimension: key.0,
                            pos: key.1,
                            stacks,
                        });
                    }
                }
                for block in broke {
                    self.multiblock_broken(i, &block);
                }
            }
            if !live.is_empty() {
                self.call(i, FUEL_TICK, |run| match &run.blocks {
                    Some((h, _)) => {
                        h.horizon_mods_block_entity_events()
                            .call_tick(&mut run.store, tick, &live)
                    }
                    None => Ok(()),
                });
            }
            self.redstone_pass(i, &id);
            // Every CHECK_EVERY ticks, the mod's watched structures are
            // matched against the world again (capability `multiblocks`).
            if self.entries[i]
                .run
                .as_ref()
                .is_some_and(|r| r.multiblocks.is_some())
                && tick.is_multiple_of(CHECK_EVERY)
            {
                self.multiblock_pass(i, &id);
            }
        }
    }

    /// `multiblock-events.broken` in mod `i`.
    fn multiblock_broken(&mut self, i: usize, block: &Multiblock) {
        self.call(i, FUEL_EVENT, |run| match &run.multiblocks {
            Some(h) => h
                .horizon_mods_multiblock_events()
                .call_broken(&mut run.store, block),
            None => Ok(()),
        });
    }

    /// One check pass of mod `id`'s watches (capability `multiblocks`):
    /// each is matched against the world again, from where the last pass
    /// stopped, within [`CHECK_READS`] block reads — a watch the budget
    /// doesn't reach waits for the next pass. The world and the store must
    /// be lent (this runs inside [`Mods::block_entity_tick`]).
    fn multiblock_pass(&mut self, i: usize, id: &str) {
        let Some(table) = self.multiblocks.lock().ok().and_then(|s| s.clone()) else {
            return;
        };
        let mut events: Vec<(bool, Multiblock)> = Vec::new();
        {
            let Ok(world) = self.world.lock() else {
                return;
            };
            let Some(world) = world.as_ref() else {
                return;
            };
            let Ok(mut slot) = self.entities.lock() else {
                return;
            };
            let Some(store) = slot.as_mut() else {
                return;
            };
            let watched: Vec<(crate::block_entities::Key, String)> = store
                .multiblocks
                .of(id)
                .map(|((d, p), s, _)| ((d.clone(), *p), s.to_owned()))
                .collect();
            if watched.is_empty() {
                return;
            }
            let len = watched.len();
            let mut reads = 0;
            let mut next = store.multiblocks.cursor % len;
            for _ in 0..len {
                let (key, structure_id) = &watched[next];
                let Some(structure) = table.of_mod(id).iter().find(|s| s.id == *structure_id)
                else {
                    // A structure the mod no longer carries: kept, skipped.
                    next = (next + 1) % len;
                    continue;
                };
                // The budget counts the cells a check reads; a watch past
                // it waits for the next pass, which starts here.
                if reads + structure.cells.len() > CHECK_READS {
                    break;
                }
                // The anchor's block entity must be there, and its chunk
                // loaded, as the anchor's block stands.
                let Some(data) = store.get(id, key).map(<[u8]>::to_vec) else {
                    next = (next + 1) % len;
                    continue;
                };
                let Some(state) = world.block(&key.0, key.1[0], key.1[1], key.1[2]) else {
                    next = (next + 1) % len;
                    continue;
                };
                let watch = store
                    .multiblocks
                    .get(id, key, structure_id)
                    .unwrap_or(crate::multiblocks::Watch::UNTOLD);
                let check = |quarter: u32| {
                    crate::multiblocks::check_cells(structure, key.1, quarter, &|bx, by, bz| {
                        world.block(&key.0, bx, by, bz)
                    })
                };
                // The quarter it last held first, then the others: a core
                // rebuilt turned is whole where it stands.
                let mut whole = None;
                for (i, quarter) in [watch.quarter, 0, 1, 2, 3].into_iter().enumerate() {
                    if whole.is_some() || (i > 0 && quarter == watch.quarter) {
                        continue;
                    }
                    reads += structure.cells.len();
                    if matches!(check(quarter), Ok(None)) {
                        whole = Some(quarter);
                    }
                }
                let of = |quarter: u32| Multiblock {
                    dimension: key.0.clone(),
                    x: key.1[0],
                    y: key.1[1],
                    z: key.1[2],
                    state: state.clone(),
                    data: data.clone(),
                    structure: structure_id.clone(),
                    quarter,
                };
                let formed = |quarter: u32, told: bool| crate::multiblocks::Watch {
                    quarter,
                    formed: true,
                    told,
                };
                match (watch.formed, whole) {
                    // It came together.
                    (false, Some(q)) => {
                        events.push((true, of(q)));
                        store
                            .multiblocks
                            .set(id, key, structure_id, formed(q, true));
                    }
                    // It no longer holds.
                    (true, None) => {
                        events.push((false, of(watch.quarter)));
                        store.multiblocks.set(
                            id,
                            key,
                            structure_id,
                            crate::multiblocks::Watch {
                                quarter: watch.quarter,
                                formed: false,
                                told: false,
                            },
                        );
                    }
                    // Whole at watch time and not yet told: this pass tells
                    // it. Rebuilt turned, it only turns.
                    (true, Some(q)) if !watch.told => {
                        events.push((true, of(q)));
                        store
                            .multiblocks
                            .set(id, key, structure_id, formed(q, true));
                    }
                    (true, Some(q)) => {
                        store
                            .multiblocks
                            .set(id, key, structure_id, formed(q, true));
                    }
                    (false, None) => {}
                }
                next = (next + 1) % len;
            }
            store.multiblocks.cursor = next;
        }
        for (formed, block) in events {
            if formed {
                self.call(i, FUEL_EVENT, |run| match &run.multiblocks {
                    Some(h) => h
                        .horizon_mods_multiblock_events()
                        .call_formed(&mut run.store, &block),
                    None => Ok(()),
                });
            } else {
                self.multiblock_broken(i, &block);
            }
        }
    }
    /// Each running mod taking the use of some of its blocks: its id and
    /// those blocks.
    pub fn block_use_claims(&self) -> Vec<(String, Vec<String>)> {
        self.entries
            .iter()
            .filter_map(|e| {
                let run = e.run.as_ref()?;
                let (_, blocks) = run.uses.as_ref()?;
                Some((run.store.data().id.clone(), blocks.clone()))
            })
            .collect()
    }
    /// `player` used block `state` at `pos` in `dimension`, one mod
    /// `mod_id` takes the use of: its block entity there (made now if it has
    /// none, a block placed before the mod ran) goes to `used`. Needs the
    /// store lent.
    pub fn block_used(
        &mut self,
        mod_id: &str,
        player: &ServerPlayer,
        dimension: &str,
        pos: [i32; 3],
        state: &str,
    ) {
        let Some(i) = self.running_index(mod_id) else {
            return;
        };
        let key = (dimension.to_owned(), pos);
        let data = {
            let Ok(mut slot) = self.entities.lock() else {
                return;
            };
            let Some(store) = slot.as_mut() else {
                return;
            };
            store.create(mod_id, key.clone());
            store
                .get(mod_id, &key)
                .map(<[u8]>::to_vec)
                .unwrap_or_default()
        };
        let block = UsedBlock {
            dimension: dimension.to_owned(),
            x: pos[0],
            y: pos[1],
            z: pos[2],
            state: state.to_owned(),
            data,
        };
        self.call(i, FUEL_EVENT, |run| match &run.uses {
            Some((h, _)) => {
                h.horizon_mods_block_use_events()
                    .call_used(&mut run.store, player, &block)
            }
            None => Ok(()),
        });
    }
    /// Each `CONFIG_POLL` ticks: looks at the config files (capability
    /// `config`) and tells the mods what a hand edit changed.
    pub fn config_tick(&mut self, tick: u64) {
        if tick.is_multiple_of(CONFIG_POLL) {
            self.config_reload();
        }
    }
    /// Reads each running mod's config file now when it differs from what
    /// the mod last saw (a value put right is written back), and tells the
    /// mod which entries changed.
    pub fn config_reload(&mut self) {
        for i in 0..self.entries.len() {
            let Some(run) = self.entries[i].run.as_mut() else {
                continue;
            };
            let state = run.store.data_mut();
            if state.config.entries().is_empty() {
                continue;
            }
            let text = std::fs::read_to_string(&state.config_path).ok();
            let reload = state.config.read(text.as_deref());
            state.apply_config_reload(&reload);
            if reload.changed.is_empty() {
                continue;
            }
            self.call(i, FUEL_EVENT, |run| match &run.config {
                Some(h) => h
                    .horizon_mods_config_events()
                    .call_changed(&mut run.store, &reload.changed),
                None => Ok(()),
            });
        }
    }
    /// Tells every running mod with capability `game-events` of an event.
    fn game_event(
        &mut self,
        call: impl Fn(&GameEventsHost, &mut Store<State>) -> wasmtime::Result<()>,
    ) {
        for i in 0..self.entries.len() {
            if self.entries[i]
                .run
                .as_ref()
                .is_none_or(|r| r.game_events.is_none())
            {
                continue;
            }
            self.call(i, FUEL_EVENT, |run| match &run.game_events {
                Some(h) => call(h, &mut run.store),
                None => Ok(()),
            });
        }
    }
    /// Whether a running mod hears game events.
    pub fn wants_game_events(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.run.as_ref().is_some_and(|r| r.game_events.is_some()))
    }
    /// `player` lost `amount` health, to `health`.
    pub fn player_hurt(&mut self, player: &ServerPlayer, amount: f32, health: f32) {
        self.game_event(|h, s| {
            h.horizon_mods_game_events()
                .call_player_hurt(s, player, amount, health)
        });
    }
    /// `player` died; `cause` is the death message.
    pub fn player_died(&mut self, player: &ServerPlayer, cause: &str) {
        self.game_event(|h, s| {
            h.horizon_mods_game_events()
                .call_player_died(s, player, cause)
        });
    }
    /// `player` is alive again.
    pub fn player_respawned(&mut self, player: &ServerPlayer) {
        self.game_event(|h, s| {
            h.horizon_mods_game_events()
                .call_player_respawned(s, player)
        });
    }
    /// `player` is in dimension `to`, after `from`.
    pub fn dimension_changed(&mut self, player: &ServerPlayer, from: &str, to: &str) {
        self.game_event(|h, s| {
            h.horizon_mods_game_events()
                .call_dimension_changed(s, player, from, to)
        });
    }
    /// `player`'s game mode changed.
    pub fn game_mode_changed(&mut self, player: &ServerPlayer, from: GameMode, to: GameMode) {
        self.game_event(|h, s| {
            h.horizon_mods_game_events()
                .call_game_mode_changed(s, player, from, to)
        });
    }
    /// `player` killed mob `id` of `kind`.
    pub fn mob_killed(&mut self, player: &ServerPlayer, kind: &str, id: u32) {
        self.game_event(|h, s| {
            h.horizon_mods_game_events()
                .call_mob_killed(s, player, kind, id)
        });
    }
    /// The weather turned to `weather` (`clear`, `rain`, `thunder`).
    pub fn weather_changed(&mut self, weather: &str) {
        self.game_event(|h, s| {
            h.horizon_mods_game_events()
                .call_weather_changed(s, weather)
        });
    }
    /// Whether a running mod has capability `redstone`: only then are the
    /// signals around its blocks worth reading.
    pub fn wants_redstone(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.run.as_ref().is_some_and(|r| r.redstone.is_some()))
    }
    /// The ids of the running mods that hear redstone changes.
    pub fn redstone_mods(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|e| e.run.as_ref())
            .filter(|r| r.redstone.is_some())
            .map(|r| r.store.data().id.clone())
            .collect()
    }
    /// Tells mod `i` (id `id`) of the block entities whose reading changed:
    /// `input-changed`, at most [`REDSTONE_PER_TICK`] at a time.
    fn redstone_pass(&mut self, i: usize, id: &str) {
        if self.entries[i]
            .run
            .as_ref()
            .is_none_or(|r| r.redstone.is_none())
        {
            return;
        }
        let changed = {
            let Ok(mut slot) = self.entities.lock() else {
                return;
            };
            let Some(store) = slot.as_mut() else {
                return;
            };
            let keys: Vec<crate::block_entities::Key> =
                store.of(id).map(|(k, _)| k.clone()).collect();
            let mut changed = store.redstone.changed_inputs(keys.iter());
            if changed.len() > REDSTONE_PER_TICK {
                // The rest wait: forget that they were told.
                for (key, _) in changed.split_off(REDSTONE_PER_TICK) {
                    store.redstone.told.remove(&key);
                }
            }
            changed
        };
        for ((dimension, [x, y, z]), input) in changed {
            let input = sides_to_wit(input);
            self.call(i, FUEL_EVENT, |run| match &run.redstone {
                Some(h) => h.horizon_mods_redstone_events().call_input_changed(
                    &mut run.store,
                    &dimension,
                    x,
                    y,
                    z,
                    input,
                ),
                None => Ok(()),
            });
        }
    }
    /// Lends the game's mobs to capability `entities` for the calls until
    /// [`Mods::take_entities`]; with none lent, its functions fail.
    pub fn lend_entities(&mut self, state: EntityState) {
        if let Ok(mut slot) = self.mod_entities.lock() {
            *slot = Some(state);
        }
    }
    /// The state [`Mods::lend_entities`] lent, back to the game (its
    /// `next_id` says how many ids the mods promised).
    pub fn take_entities(&mut self) -> Option<EntityState> {
        self.mod_entities.lock().ok()?.take()
    }
    /// Whether a running mod may spawn and steer mobs: only then are the
    /// game's mobs worth scanning for marks.
    pub fn wants_entities(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.run.as_ref().is_some_and(|r| r.entities.is_some()))
    }
    /// Whether mod `mod_id` runs here and hears events of its mobs.
    pub fn hears_entities(&self, mod_id: &str) -> bool {
        self.running_index(mod_id)
            .and_then(|i| self.entries[i].run.as_ref())
            .is_some_and(|r| r.entities.is_some())
    }
    /// Takes the outputs of capability `entities` out of the queue, leaving
    /// the others: for a game that applies mobs between calls (the test
    /// game).
    pub fn take_entity_outputs(&mut self) -> Vec<Output> {
        let (mine, rest): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.out).into_iter().partition(|o| {
                matches!(
                    o,
                    Output::SpawnEntity { .. }
                        | Output::TeleportEntity { .. }
                        | Output::PushEntity { .. }
                        | Output::SetEntityHealth { .. }
                        | Output::RemoveEntity { .. }
                )
            });
        self.out = rest;
        mine
    }
    /// Each server tick: every mod told of its own mobs (`entities` is all
    /// the mods' mobs).
    pub fn entity_tick(&mut self, tick: u64, entities: &[EntityInfo]) {
        for i in 0..self.entries.len() {
            let Some(run) = self.entries[i].run.as_ref() else {
                continue;
            };
            if run.entities.is_none() {
                continue;
            }
            let id = run.store.data().id.clone();
            let own: Vec<entities::Entity> = entities
                .iter()
                .filter(|e| e.owner == id)
                .take(ENTITIES_PER_TICK)
                .map(EntityInfo::to_wit)
                .collect();
            if own.is_empty() {
                continue;
            }
            self.call(i, FUEL_TICK, |run| match &run.entities {
                Some(h) => h
                    .horizon_mods_entity_events()
                    .call_tick(&mut run.store, tick, &own),
                None => Ok(()),
            });
        }
    }
    /// A player's blow is about to hurt `entity`: the damage its mod answers
    /// (never over four times `damage`, never below 0); `damage` when the
    /// mod doesn't hear or fails.
    pub fn entity_hurt(&mut self, player: &ServerPlayer, entity: &EntityInfo, damage: f32) -> f32 {
        let Some(i) = self.running_index(&entity.owner) else {
            return damage;
        };
        let wit = entity.to_wit();
        let answered = self.call(i, FUEL_EVENT, |run| match &run.entities {
            Some(h) => {
                h.horizon_mods_entity_events()
                    .call_hurt(&mut run.store, player, &wit, damage)
            }
            None => Ok(Ok(damage)),
        });
        match answered {
            Some(Ok(d)) if d.is_finite() => d.clamp(0.0, damage * 4.0),
            _ => damage,
        }
    }
    /// `entity` died.
    pub fn entity_died(&mut self, entity: &EntityInfo) {
        let Some(i) = self.running_index(&entity.owner) else {
            return;
        };
        let wit = entity.to_wit();
        self.call(i, FUEL_EVENT, |run| match &run.entities {
            Some(h) => h
                .horizon_mods_entity_events()
                .call_died(&mut run.store, &wit),
            None => Ok(()),
        });
    }
    /// `entity` is gone; it is its last state.
    pub fn entity_removed(&mut self, entity: &EntityInfo) {
        let Some(i) = self.running_index(&entity.owner) else {
            return;
        };
        let wit = entity.to_wit();
        self.call(i, FUEL_EVENT, |run| match &run.entities {
            Some(h) => h
                .horizon_mods_entity_events()
                .call_removed(&mut run.store, &wit),
            None => Ok(()),
        });
    }
    /// `player` right-clicked `entity`.
    pub fn entity_used(&mut self, player: &ServerPlayer, entity: &EntityInfo) {
        let Some(i) = self.running_index(&entity.owner) else {
            return;
        };
        let wit = entity.to_wit();
        self.call(i, FUEL_EVENT, |run| match &run.entities {
            Some(h) => h
                .horizon_mods_entity_events()
                .call_used(&mut run.store, player, &wit),
            None => Ok(()),
        });
    }
    /// The client half of mod `mod_id` sent `data` on `channel` (the game
    /// has read and checked the message): the server mod hears it.
    pub fn net_client_sent(
        &mut self,
        mod_id: &str,
        player: &ServerPlayer,
        channel: &str,
        data: &[u8],
    ) {
        let Some(i) = self.running_index(mod_id) else {
            return;
        };
        let data = data.to_vec();
        self.call(i, FUEL_EVENT, |run| match &run.net_server {
            Some(h) => h.horizon_mods_network_events().call_client_sent(
                &mut run.store,
                player,
                channel,
                &data,
            ),
            None => Ok(()),
        });
    }
    /// The server half of mod `mod_id` sent `data` on `channel`: the client
    /// mod hears it.
    pub fn net_server_sent(&mut self, mod_id: &str, channel: &str, data: &[u8]) {
        let Some(i) = self.running_index(mod_id) else {
            return;
        };
        let data = data.to_vec();
        self.call(i, FUEL_EVENT, |run| match &run.net_client {
            Some(h) => h.horizon_mods_network_client_events().call_server_sent(
                &mut run.store,
                channel,
                &data,
            ),
            None => Ok(()),
        });
    }
    /// Whether mod `mod_id` runs here and hears messages.
    pub fn hears_net(&self, mod_id: &str) -> bool {
        self.running_index(mod_id).is_some_and(|i| {
            self.entries[i]
                .run
                .as_ref()
                .is_some_and(|r| r.net_server.is_some() || r.net_client.is_some())
        })
    }
    /// `player` clicked slot `slot` of mod `mod_id`'s menu `menu`.
    pub fn menu_clicked(
        &mut self,
        mod_id: &str,
        player: &ServerPlayer,
        menu: &str,
        slot: u32,
        button: u8,
        shift: bool,
    ) {
        let Some(i) = self.running_index(mod_id) else {
            return;
        };
        self.call(i, FUEL_EVENT, |run| match &run.menus {
            Some(h) => h.horizon_mods_menu_events().call_clicked(
                &mut run.store,
                player,
                menu,
                slot,
                button,
                shift,
            ),
            None => Ok(()),
        });
    }
    /// `player` closed mod `mod_id`'s menu `menu`.
    pub fn menu_closed(&mut self, mod_id: &str, player: &ServerPlayer, menu: &str) {
        let Some(i) = self.running_index(mod_id) else {
            return;
        };
        self.call(i, FUEL_EVENT, |run| match &run.menus {
            Some(h) => h
                .horizon_mods_menu_events()
                .call_closed(&mut run.store, player, menu),
            None => Ok(()),
        });
    }
    /// Each running mod taking the behaviour of some items (capability
    /// `item-hooks`): its id and those items.
    pub fn item_use_claims(&self) -> Vec<(String, Vec<String>)> {
        self.entries
            .iter()
            .filter_map(|e| {
                let run = e.run.as_ref()?;
                let (_, items) = run.items.as_ref()?;
                Some((run.store.data().id.clone(), items.clone()))
            })
            .collect()
    }
    /// `player` used (right-clicked) the stack in the main hand — `stack`,
    /// with the mod's own bytes on it — one mod `mod_id` claims (capability
    /// `item-hooks`): `used`, or `used-on-block` where the crosshair was on
    /// `block` (its position in the player's dimension and state). The mod
    /// hears it at the tick's end.
    pub fn item_used(
        &mut self,
        mod_id: &str,
        player: &ServerPlayer,
        stack: HeldStack,
        block: Option<([i32; 3], &str)>,
    ) {
        let Some(i) = self.running_index(mod_id) else {
            return;
        };
        let block = block.map(|(pos, state)| ItemUsedBlock {
            dimension: player.dimension.clone(),
            x: pos[0],
            y: pos[1],
            z: pos[2],
            state: state.to_owned(),
        });
        self.call(i, FUEL_EVENT, |run| match &run.items {
            Some((h, _)) => match &block {
                Some(b) => h.horizon_mods_item_events().call_used_on_block(
                    &mut run.store,
                    player,
                    &stack,
                    b,
                ),
                None => h
                    .horizon_mods_item_events()
                    .call_used(&mut run.store, player, &stack),
            },
            None => Ok(()),
        });
    }
    /// Each server tick, after [`Mods::block_entity_tick`]: every claimed
    /// stack of every online player to the mod that claims it, held or worn
    /// (`inventory-tick`). The game collects the lists, at most
    /// [`TICK_ITEMS`] stacks a mod.
    pub fn player_items_tick(&mut self, tick: u64, held: &[(String, Vec<PlayerStack>)]) {
        for (id, stacks) in held {
            let Some(i) = self.running_index(id) else {
                continue;
            };
            if stacks.is_empty() {
                continue;
            }
            self.call(i, FUEL_TICK, |run| match &run.items {
                Some((h, _)) => {
                    h.horizon_mods_item_events()
                        .call_inventory_tick(&mut run.store, tick, stacks)
                }
                None => Ok(()),
            });
        }
    }
    /// The player holding one of mod `mod_id`'s claimed items in the main
    /// hand struck `entity` for `damage`: the mod's answer is the damage to
    /// deal instead, kept to `0.0` and four times vanilla's; an error (or a
    /// trap, which turns the mod off) keeps vanilla's.
    pub fn item_hurt_entity(
        &mut self,
        mod_id: &str,
        player: &ServerPlayer,
        stack: HeldStack,
        entity: &ItemEntity,
        damage: f32,
    ) -> f32 {
        let Some(i) = self.running_index(mod_id) else {
            return damage;
        };
        let answer = self.call(i, FUEL_EVENT, |run| match &run.items {
            Some((h, _)) => h.horizon_mods_item_events().call_hurt_entity(
                &mut run.store,
                player,
                &stack,
                entity,
                damage,
            ),
            None => Ok(Ok(damage)),
        });
        match answer {
            Some(Ok(n)) if n.is_finite() => n.clamp(0.0, damage * 4.0),
            _ => damage,
        }
    }
    /// `player` was hurt for `damage` of `source`: each mod claiming items
    /// answers with how much of it its armour takes, and what the answers
    /// take together is gone (kept to `damage`; an error takes nothing).
    pub fn item_hurt(&mut self, player: &ServerPlayer, damage: f32, source: &str) -> f32 {
        let mut taken = 0.0;
        for i in 0..self.entries.len() {
            if self.entries[i]
                .run
                .as_ref()
                .is_none_or(|r| r.items.is_none())
            {
                continue;
            }
            let answer = self.call(i, FUEL_EVENT, |run| match &run.items {
                Some((h, _)) => {
                    h.horizon_mods_item_events()
                        .call_hurt(&mut run.store, player, damage, source)
                }
                None => Ok(Ok(0.0)),
            });
            if let Some(Ok(n)) = answer
                && n.is_finite()
            {
                taken += n.clamp(0.0, damage);
            }
        }
        taken.min(damage)
    }
    /// Whether mod `mod_id` runs here.
    pub fn runs(&self, mod_id: &str) -> bool {
        self.running_index(mod_id).is_some()
    }
    /// The entry of running mod `mod_id`.
    fn running_index(&self, mod_id: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.run.as_ref().is_some_and(|r| r.store.data().id == mod_id))
    }
    /// Mod `id`'s block entities in loaded chunks: those whose block is still
    /// one it claims, and those whose block is gone (removed from the store
    /// here). `None` when the world or the store isn't lent.
    fn sort_block_entities(
        &self,
        id: &str,
        claimed: &[String],
    ) -> Option<(Vec<BlockEntity>, Vec<BlockEntity>)> {
        let world = self.world.lock().ok()?;
        let world = world.as_ref()?;
        let mut entities = self.entities.lock().ok()?;
        let store = entities.as_mut()?;
        let (mut live, mut gone) = (Vec::new(), Vec::new());
        for ((dimension, [x, y, z]), data) in store.of(id) {
            // An unloaded chunk keeps its block entities, untouched.
            let Some(state) = world.block(dimension, *x, *y, *z) else {
                continue;
            };
            let entity = BlockEntity {
                dimension: dimension.clone(),
                x: *x,
                y: *y,
                z: *z,
                data: data.to_vec(),
                state,
            };
            let name = crate::block_entities::block_name(&entity.state);
            if claimed.iter().any(|c| c == name) {
                live.push(entity);
            } else {
                gone.push(entity);
            }
        }
        for e in &gone {
            store.remove(id, &(e.dimension.clone(), [e.x, e.y, e.z]));
        }
        Some((live, gone))
    }

    /// Reads every mod at `paths` and starts those `enabled` says are on,
    /// client and server mods alike. A command that `reserved` (vanilla's
    /// commands) or an earlier mod already uses is refused.
    pub fn load(
        paths: &[PathBuf],
        enabled: impl Fn(&str) -> bool,
        reserved: impl Fn(&str) -> bool,
    ) -> Mods {
        Mods::load_for(Runs::Both, paths, enabled, reserved, None, None)
    }
    /// [`Mods::load`], starting only the mods `runs` says this game runs; the
    /// others that are on are [`Status::Elsewhere`]. `recipes` is capability
    /// `machine-recipes`' table, built by the game before this call so that
    /// every mod's `init` already reads it (`None`: the mods' machines have
    /// no recipes, and `machine-recipes.all` answers empty); `multiblocks`
    /// is capability `multiblocks`' table of structure templates, built the
    /// same way (`None`: no structures, and `multiblocks.all` answers
    /// empty).
    pub fn load_for(
        runs: Runs,
        paths: &[PathBuf],
        enabled: impl Fn(&str) -> bool,
        reserved: impl Fn(&str) -> bool,
        recipes: Option<Arc<crate::machine_recipes::MachineRecipes>>,
        multiblocks: Option<Arc<crate::multiblocks::Templates>>,
    ) -> Mods {
        let mut mods = Mods::default();
        if let Some(recipes) = recipes
            && let Ok(mut slot) = mods.machine_recipes.lock()
        {
            *slot = Some(recipes);
        }
        if let Some(multiblocks) = multiblocks
            && let Ok(mut slot) = mods.multiblocks.lock()
        {
            *slot = Some(multiblocks);
        }
        for path in paths {
            let modified = modified(path);
            let mut entry = Entry {
                path: path.clone(),
                manifest: None,
                status: Status::Off,
                cost: Duration::ZERO,
                run: None,
                modified,
            };
            match Package::read(path) {
                Err(e) => entry.status = Status::Failed(e),
                Ok(package) => {
                    let on = enabled(&package.manifest.id);
                    entry.manifest = Some(package.manifest.clone());
                    if on && !runs.starts(package.manifest.side) {
                        entry.status = Status::Elsewhere;
                    } else if on {
                        // Content is read before the world loads; a mod whose
                        // content is refused stays off, with the reason.
                        let content = crate::Content::read(path, &package.manifest);
                        entry.status =
                            match content.and_then(|_| mods.start(path, &package, &reserved)) {
                                Ok(run) => {
                                    entry.run = run;
                                    Status::Running
                                }
                                Err(e) => Status::Failed(e),
                            };
                    }
                }
            }
            if let (Status::Failed(reason), Some(m)) = (&entry.status, &entry.manifest) {
                log::warn!("Horizon: mod {} did not start: {reason}", m.id);
            }
            mods.entries.push(entry);
        }
        mods
    }

    fn engine(&mut self) -> Result<&(Engine, Linker<State>), String> {
        if self.engine.is_none() {
            let mut config = Config::new();
            config.wasm_component_model(true).consume_fuel(true);
            let engine = Engine::new(&config).map_err(|e| e.to_string())?;
            let mut linker = Linker::new(&engine);
            HorizonMod::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            players::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            mod_world::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            UiHost::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            RenderHost::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            RecipesHost::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            BlockEntityHost::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            menus::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            displays::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            machine_recipes::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            multiblocks::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            saved_data::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            item_hooks::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            network::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            entities::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            redstone::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            mod_config::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            effects::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                .map_err(|e| e.to_string())?;
            self.engine = Some((engine, linker));
        }
        Ok(self.engine.as_ref().expect("made above"))
    }

    /// Starts a mod's code; `Ok(None)` for a content-only mod, which has
    /// nothing to run (the game reads its content before the world loads).
    /// `path` is the mod's own `.hmod` or folder, for capability `storage`'s
    /// per-mod directory.
    fn start(
        &mut self,
        path: &Path,
        package: &Package,
        reserved: &dyn Fn(&str) -> bool,
    ) -> Result<Option<Running>, String> {
        let m = &package.manifest;
        let Some(wasm) = &package.wasm else {
            return Ok(None);
        };
        // Two running mods with the same id would share one `data_dir`
        // (capability `storage`): the first loaded keeps the id.
        if self
            .entries
            .iter()
            .any(|e| e.run.is_some() && e.manifest.as_ref().is_some_and(|other| other.id == m.id))
        {
            return Err(format!("another running mod already uses id {}", m.id));
        }
        let server = m.side == Side::Server;
        let taken: Vec<String> = self
            .entries
            .iter()
            .filter_map(|e| e.run.as_ref())
            .filter(|r| r.hooks.server().is_some() == server)
            .flat_map(|r| r.hooks.commands())
            .map(str::to_owned)
            .collect();
        let snapshot = self.snapshot.clone();
        let world = self.world.clone();
        let entities = self.entities.clone();
        let machine_recipes = self.machine_recipes.clone();
        let multiblocks = self.multiblocks.clone();
        let player_items = self.player_items.clone();
        let mod_entities = self.mod_entities.clone();
        let (engine, linker) = self.engine()?;
        let component =
            Component::new(engine, wasm).map_err(|e| format!("not a component: {e}"))?;
        let ty = component.component_type();
        // An interface the component imports with none of its functions —
        // the shape an exported interface's shared types leave in it
        // (`item-events` uses `item-hooks`' `held-stack`) — asks for no
        // capability: a capability is what a mod calls.
        let imports: Vec<String> = ty
            .imports(engine)
            .filter(|(_, item)| match &item.ty {
                wasmtime::component::types::ComponentItem::ComponentInstance(i) => {
                    i.exports(engine).any(|(_, e)| {
                        matches!(
                            e.ty,
                            wasmtime::component::types::ComponentItem::ComponentFunc(_)
                        )
                    })
                }
                _ => true,
            })
            .map(|(name, _)| name.to_string())
            .collect();
        m.check_imports(imports.iter().map(String::as_str))?;
        // A function this Horizon doesn't have: say so, not wasmtime's linking error.
        let functions = crate::api_level::imports(wasm)?;
        let (level, function) = crate::api_level::needed(functions.iter().map(String::as_str));
        if let Some(why) = crate::api_level::too_new(level, function) {
            return Err(why);
        }
        let config_path = path
            .parent()
            .unwrap_or(Path::new("."))
            .join(crate::config::DIR)
            .join(format!("{}.toml", m.id));
        let data_dir = path
            .parent()
            .unwrap_or(Path::new("."))
            .join(crate::manifest::STORAGE_DIR)
            .join(&m.id);
        let state = State {
            name: m.name.clone(),
            id: m.id.clone(),
            snapshot,
            out: Vec::new(),
            panel: None,
            ui: UiState::default(),
            shapes: Vec::new(),
            corner: None,
            zoom: None,
            minimap: None,
            far_terrain: None,
            input: InputState::default(),
            browser: BrowserState::default(),
            data_dir,
            storage_budget: STORAGE_WRITE_BUDGET,
            limits: StoreLimitsBuilder::new()
                .memory_size(MEMORY)
                .instances(16)
                .build(),
            world,
            entities,
            machine_recipes,
            multiblocks,
            player_items,
            item_claims: Vec::new(),
            item_writes: 0,
            server,
            net_sent: (0, 0),
            mod_entities,
            entity_spawns: 0,
            effects_sent: 0,
            config: Default::default(),
            config_path,
            config_writes: 0,
        };
        let mut store = Store::new(engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_fuel(FUEL_INIT).map_err(|e| e.to_string())?;
        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(|e| reason(&e))?;
        let (mut hooks, ui) = if server {
            let exports = HorizonServerMod::new(&mut store, &instance).map_err(|e| reason(&e))?;
            let r = exports
                .horizon_mods_server_events()
                .call_init(&mut store)
                .map_err(|e| reason(&e))??;
            (Hooks::Server(exports, r), None)
        } else {
            let exports = HorizonMod::new(&mut store, &instance).map_err(|e| reason(&e))?;
            // `ui-events` is a mod built since `ui`'s; older mods have no export
            // and stay callable through `events` alone.
            let ui = UiHost::new(&mut store, &instance).ok();
            let r = exports
                .horizon_mods_events()
                .call_init(&mut store)
                .map_err(|e| reason(&e))??;
            (Hooks::Client(exports, r), ui)
        };
        let free = |c: &str| {
            let free = !reserved(c) && !taken.iter().any(|t| t == c);
            if !free {
                log::warn!("Horizon: mod {} cannot take /{c}: already a command", m.id);
            }
            free
        };
        match &mut hooks {
            Hooks::Client(_, r) => r.commands.retain(|c| free(c)),
            Hooks::Server(_, r) => {
                r.commands.retain(|c| free(&c.name));
                for c in &mut r.commands {
                    if let Err(e) = check_arguments(&c.arguments) {
                        log::warn!(
                            "Horizon: mod {}'s /{}: {e}; it takes free text",
                            m.id,
                            c.name
                        );
                        c.arguments.clear();
                    }
                }
            }
        }
        // `block-entity-events` is a server mod built since capability
        // `block-entities`; its claims count only with the capability.
        let mut blocks = None;
        if server
            && m.capabilities.contains(&crate::Capability::BlockEntities)
            && let Ok(host) = BlockEntityHost::new(&mut store, &instance)
        {
            store.set_fuel(FUEL_INIT).map_err(|e| e.to_string())?;
            let mut claimed = host
                .horizon_mods_block_entity_events()
                .call_blocks(&mut store)
                .map_err(|e| reason(&e))?;
            claimed.truncate(CLAIMED_BLOCKS);
            claimed.retain(|b| b.contains(':') && !b.contains('['));
            if !claimed.is_empty() {
                blocks = Some((host, claimed));
            }
        }
        // `block-use-events`: among the claimed blocks, those it takes the
        // use of.
        let mut uses = None;
        if let Some((_, claimed)) = &blocks
            && let Ok(host) = UseHost::new(&mut store, &instance)
        {
            store.set_fuel(FUEL_INIT).map_err(|e| e.to_string())?;
            let mut taken = host
                .horizon_mods_block_use_events()
                .call_uses(&mut store)
                .map_err(|e| reason(&e))?;
            taken.retain(|b| claimed.contains(b));
            if !taken.is_empty() {
                uses = Some((host, taken));
            }
        }
        let menus = match server && m.capabilities.contains(&crate::Capability::Menus) {
            true => MenuHost::new(&mut store, &instance).ok(),
            false => None,
        };
        // `multiblock-events`: a server mod built before the capability has
        // no export and keeps working, watching nothing.
        let multiblocks = match server && m.capabilities.contains(&crate::Capability::Multiblocks) {
            true => MultiblockHost::new(&mut store, &instance).ok(),
            false => None,
        };
        // `item-events` (capability `item-hooks`): a server mod built before
        // the capability has no export and keeps working, claiming no items.
        let mut items = None;
        if server
            && m.capabilities.contains(&crate::Capability::ItemHooks)
            && let Ok(host) = ItemHost::new(&mut store, &instance)
        {
            store.set_fuel(FUEL_INIT).map_err(|e| e.to_string())?;
            let mut claimed = host
                .horizon_mods_item_events()
                .call_items(&mut store)
                .map_err(|e| reason(&e))?;
            claimed.truncate(CLAIMED_ITEMS);
            claimed.retain(|i| i.contains(':') && !i.contains('['));
            if !claimed.is_empty() {
                store.data_mut().item_claims = claimed.clone();
                items = Some((host, claimed));
            }
        }
        // `entity-events`: a server mod built before the capability has no
        // export and keeps working, hearing nothing of its mobs.
        let entity_export = match server && m.capabilities.contains(&crate::Capability::Entities) {
            true => EntityHost::new(&mut store, &instance).ok(),
            false => None,
        };
        // `config-events`: a server mod built before the capability has no
        // export and keeps working, hearing nothing of edits.
        let config_export = match server && m.capabilities.contains(&crate::Capability::Config) {
            true => ConfigHost::new(&mut store, &instance).ok(),
            false => None,
        };
        // `game-events`: a server mod built before the capability has no
        // export and keeps working, hearing nothing.
        let game_events_export =
            match server && m.capabilities.contains(&crate::Capability::GameEvents) {
                true => GameEventsHost::new(&mut store, &instance).ok(),
                false => None,
            };
        // `redstone-events`: a server mod built before the capability has no
        // export and keeps working, hearing nothing of its signals.
        let redstone_export = match server && m.capabilities.contains(&crate::Capability::Redstone)
        {
            true => RedstoneHost::new(&mut store, &instance).ok(),
            false => None,
        };
        // `network-events` / `network-client-events`: a mod built before the
        // capability has no export and keeps working, hearing nothing.
        let wants_net = m.capabilities.contains(&crate::Capability::Network);
        let net_server = match server && wants_net {
            true => NetServerHost::new(&mut store, &instance).ok(),
            false => None,
        };
        let net_client = match !server && wants_net {
            true => NetClientHost::new(&mut store, &instance).ok(),
            false => None,
        };
        self.generation += 1;
        self.out.append(&mut store.data_mut().out);
        Ok(Some(Running {
            store,
            ui,
            hooks,
            raycasts: functions.iter().any(|f| f == "game.raycast"),
            reads_recipes: functions.iter().any(|f| f.starts_with("recipes.")),
            reads_machine_recipes: functions.iter().any(|f| f.starts_with("machine-recipes.")),
            blocks,
            menus,
            uses,
            multiblocks,
            items,
            net_server,
            net_client,
            entities: entity_export,
            redstone: redstone_export,
            game_events: game_events_export,
            config: config_export,
        }))
    }

    /// True when at least one mod runs: the game skips its mod work otherwise.
    pub fn is_running(&self) -> bool {
        self.entries.iter().any(|e| e.run.is_some())
    }
    /// True when a server mod runs: the server skips its mod work otherwise.
    pub fn runs_server(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.run.as_ref().is_some_and(|r| r.hooks.server().is_some()))
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    /// What mods read until the next snapshot.
    pub fn set_snapshot(&mut self, snapshot: Snapshot) {
        self.snapshot = Arc::new(snapshot);
    }

    /// Calls `f` on every running mod whose hooks `wants`, with `fuel`.
    fn each(
        &mut self,
        fuel: u64,
        wants: impl Fn(&Hooks) -> bool,
        mut f: impl FnMut(&mut Running) -> wasmtime::Result<()>,
    ) {
        for i in 0..self.entries.len() {
            if self.entries[i]
                .run
                .as_ref()
                .is_some_and(|r| wants(&r.hooks))
            {
                self.call(i, fuel, &mut f);
            }
        }
    }
    /// [`Mods::each`] over the client mods.
    fn each_client(
        &mut self,
        fuel: u64,
        wants: impl Fn(&Registration) -> bool,
        mut f: impl FnMut(&ClientEvents, &mut Store<State>) -> wasmtime::Result<()>,
    ) {
        self.each(
            fuel,
            |h| h.client().is_some_and(&wants),
            |run| match &run.hooks {
                Hooks::Client(m, _) => f(m.horizon_mods_events(), &mut run.store),
                Hooks::Server(..) => Ok(()),
            },
        );
    }
    /// [`Mods::each`] over the server mods.
    fn each_server(
        &mut self,
        fuel: u64,
        wants: impl Fn(&ServerRegistration) -> bool,
        mut f: impl FnMut(&ServerEvents, &mut Store<State>) -> wasmtime::Result<()>,
    ) {
        self.each(
            fuel,
            |h| h.server().is_some_and(&wants),
            |run| match &run.hooks {
                Hooks::Server(m, _) => f(m.horizon_mods_server_events(), &mut run.store),
                Hooks::Client(..) => Ok(()),
            },
        );
    }

    /// One call into mod `i`; a trap or an overrun turns the mod off.
    fn call<R>(
        &mut self,
        i: usize,
        fuel: u64,
        f: impl FnOnce(&mut Running) -> wasmtime::Result<R>,
    ) -> Option<R> {
        let entry = &mut self.entries[i];
        let run = entry.run.as_mut()?;
        run.store.data_mut().snapshot = self.snapshot.clone();
        run.store.data_mut().storage_budget = STORAGE_WRITE_BUDGET;
        run.store.data_mut().item_writes = 0;
        run.store.data_mut().net_sent = (0, 0);
        run.store.data_mut().entity_spawns = 0;
        run.store.data_mut().effects_sent = 0;
        run.store.data_mut().config_writes = 0;
        let started = Instant::now();
        let result = run.store.set_fuel(fuel).and_then(|()| f(run));
        // An average over about the last 64 calls.
        entry.cost = (entry.cost * 63 + started.elapsed()) / 64;
        let data = run.store.data_mut();
        let (open, close) = (
            std::mem::take(&mut data.ui.open),
            std::mem::take(&mut data.ui.close),
        );
        self.out.append(&mut data.out);
        if open {
            self.out.push(Output::OpenScreen { owner: i });
        }
        if close {
            self.out.push(Output::CloseScreen { owner: i });
        }
        match result {
            Ok(r) => Some(r),
            Err(e) => {
                let reason = reason(&e);
                log::warn!("Horizon: mod {} stopped: {reason}", run.store.data().id);
                let name = run.store.data().name.clone();
                self.out.push(Output::Stopped {
                    name,
                    reason: reason.clone(),
                });
                entry.status = Status::Failed(reason);
                entry.run = None;
                self.generation += 1;
                None
            }
        }
    }

    /// A game tick.
    pub fn tick(&mut self, tick: u64) {
        self.each_client(FUEL_TICK, |h| h.tick, |m, s| m.call_tick(s, tick));
    }
    /// A rendered frame, `seconds` after the last.
    pub fn frame(&mut self, seconds: f32) {
        self.each_client(FUEL_FRAME, |h| h.frame, |m, s| m.call_frame(s, seconds));
    }
    /// A chat line the player received, as plain text.
    pub fn chat(&mut self, text: &str) {
        self.each_client(FUEL_EVENT, |h| h.chat, |m, s| m.call_chat(s, text));
    }
    /// Whether a running client mod owns `/name`.
    pub fn has_command(&self, name: &str) -> bool {
        self.command_owner(false, name).is_some()
    }
    fn command_owner(&self, server: bool, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| {
            e.run.as_ref().is_some_and(|r| {
                r.hooks.server().is_some() == server && r.hooks.commands().contains(&name)
            })
        })
    }
    /// Runs `line` (without its slash) if a client mod owns its command;
    /// false when none does.
    pub fn command(&mut self, line: &str) -> bool {
        let (name, args) = line.split_once(' ').unwrap_or((line, ""));
        let Some(i) = self.command_owner(false, name) else {
            return false;
        };
        let result = self.call(i, FUEL_EVENT, |run| match &run.hooks {
            Hooks::Client(m, _) => m
                .horizon_mods_events()
                .call_command(&mut run.store, name, args),
            Hooks::Server(..) => Ok(Ok(None)),
        });
        match result {
            Some(Ok(Some(text))) => self.out.push(Output::Feedback { text, error: false }),
            Some(Err(text)) => self.out.push(Output::Feedback { text, error: true }),
            Some(Ok(None)) | None => {}
        }
        true
    }
    /// The commands running client mods own, for suggestions.
    pub fn commands(&self) -> impl Iterator<Item = &str> {
        self.owned_commands(false)
    }
    fn owned_commands(&self, server: bool) -> impl Iterator<Item = &str> {
        self.entries
            .iter()
            .filter_map(|e| e.run.as_ref())
            .filter(move |r| r.hooks.server().is_some() == server)
            .flat_map(|r| r.hooks.commands())
    }

    /// A server tick.
    pub fn server_tick(&mut self, tick: u64) {
        self.each_server(FUEL_TICK, |h| h.tick, |m, s| m.call_tick(s, tick));
    }
    /// `player` joined the server (they are in the snapshot's players).
    pub fn player_joined(&mut self, player: &ServerPlayer) {
        self.each_server(
            FUEL_EVENT,
            |h| h.joins,
            |m, s| m.call_player_joined(s, player),
        );
    }
    /// `player` left the server (they are no longer in the snapshot's players).
    pub fn player_left(&mut self, player: &ServerPlayer) {
        self.each_server(
            FUEL_EVENT,
            |h| h.joins,
            |m, s| m.call_player_left(s, player),
        );
    }
    /// A chat message `player` sent, as plain text.
    pub fn player_chat(&mut self, player: &ServerPlayer, text: &str) {
        self.each_server(FUEL_EVENT, |h| h.chat, |m, s| m.call_chat(s, player, text));
    }
    /// `player` broke `state` at `pos`.
    pub fn block_broken(&mut self, player: &ServerPlayer, pos: BlockPos, state: &str) {
        self.each_server(
            FUEL_EVENT,
            |h| h.blocks,
            |m, s| m.call_block_broken(s, player, pos, state),
        );
    }
    /// `player` placed `state` at `pos`.
    pub fn block_placed(&mut self, player: &ServerPlayer, pos: BlockPos, state: &str) {
        self.each_server(
            FUEL_EVENT,
            |h| h.blocks,
            |m, s| m.call_block_placed(s, player, pos, state),
        );
    }
    /// The commands running server mods own and their arguments, for the
    /// server's command tree.
    pub fn server_commands(&self) -> impl Iterator<Item = &ServerCommand> {
        self.entries
            .iter()
            .filter_map(|e| e.run.as_ref()?.hooks.server())
            .flat_map(|r| r.commands.iter())
    }
    /// Runs `line` (without its slash), typed by `player`, if a server mod
    /// owns its command; its feedback is an [`Output::Reply`]. False when
    /// none does.
    pub fn server_command(&mut self, player: &ServerPlayer, line: &str) -> bool {
        let (name, args) = line.split_once(' ').unwrap_or((line, ""));
        let Some(i) = self.command_owner(true, name) else {
            return false;
        };
        let reply = |text, error| Output::Reply {
            player: player.id.clone(),
            text,
            error,
        };
        let arguments = self.entries[i]
            .run
            .as_ref()
            .and_then(|r| r.hooks.server())
            .and_then(|r| r.commands.iter().find(|c| c.name == name))
            .map(|c| c.arguments.as_slice())
            .unwrap_or_default();
        let values = match crate::commands::parse(arguments, args, player, &self.snapshot.players) {
            Ok(values) => values,
            Err(e) => {
                self.out.push(reply(e, true));
                return true;
            }
        };
        let result = self.call(i, FUEL_EVENT, |run| match &run.hooks {
            Hooks::Server(m, _) => m.horizon_mods_server_events().call_command(
                &mut run.store,
                player,
                name,
                args,
                &values,
            ),
            Hooks::Client(..) => Ok(Ok(None)),
        });
        match result {
            Some(Ok(Some(text))) => self.out.push(reply(text, false)),
            Some(Err(text)) => self.out.push(reply(text, true)),
            Some(Ok(None)) | None => {}
        }
        true
    }
    /// The running mods' HUD panels.
    pub fn panels(&self) -> impl Iterator<Item = &Panel> {
        self.entries
            .iter()
            .filter_map(|e| e.run.as_ref()?.store.data().panel.as_ref())
    }
    /// The running mods' corner panels (capability `render`), in load order.
    pub fn corner_panels(&self) -> impl Iterator<Item = &CornerPanel> {
        self.entries
            .iter()
            .filter_map(|e| e.run.as_ref()?.store.data().corner.as_ref())
    }
    /// Some running mod calls `game.raycast`: the snapshot needs a
    /// [`Snapshot::view_hit`].
    pub fn wants_raycast(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.run.as_ref().is_some_and(|r| r.raycasts))
    }
    /// The panel beside container screens (`browser.set-items`): the first
    /// running mod with items or recipes, its index, the panel's version and
    /// the items.
    pub fn browser_items(&self) -> Option<(usize, u64, &[BrowserStack])> {
        self.entries.iter().enumerate().find_map(|(i, e)| {
            let b = &e.run.as_ref()?.store.data().browser;
            (!b.items.is_empty() || b.recipes.is_some()).then_some((
                i,
                b.version,
                b.items.as_slice(),
            ))
        })
    }
    /// What mod `owner`'s panel shows instead of its items
    /// (`browser.set-recipes`).
    pub fn browser_recipes(&self, owner: usize) -> Option<&RecipePage> {
        let b = &self.entries.get(owner)?.run.as_ref()?.store.data().browser;
        b.recipes.as_ref()
    }
    /// The player did `event` on mod `owner`'s panel.
    pub fn browser_event(&mut self, owner: usize, event: BrowserEvent) {
        if let Some(run) = self.entries.get_mut(owner).and_then(|e| e.run.as_mut()) {
            run.store.data_mut().browser.push(event);
        }
    }
    /// The key `key` went down with the pointer on `stack` in a container
    /// screen: each mod with a key on it hears its own name for it.
    pub fn browser_key(&mut self, key: &str, stack: &BrowserStack) {
        for run in self.entries.iter_mut().filter_map(|e| e.run.as_mut()) {
            let data = run.store.data_mut();
            let names: Vec<String> = data
                .input
                .keys
                .iter()
                .filter(|k| k.key == key)
                .map(|k| k.name.clone())
                .collect();
            for key in names {
                data.browser.push(BrowserEvent::Key(BrowserItemKey {
                    key,
                    stack: stack.clone(),
                }));
            }
        }
    }
    /// Some running mod reads capability `recipes`: the snapshot needs
    /// [`Snapshot::recipes`] and [`Snapshot::known_recipes`].
    pub fn wants_recipes(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.run.as_ref().is_some_and(|r| r.reads_recipes))
    }
    /// The machine-recipes table this run was given (capability
    /// `machine-recipes`), for tests; the game passes it to
    /// [`Mods::load_for`] and it never changes while the mods run.
    pub fn machine_recipes(&self) -> Option<Arc<crate::machine_recipes::MachineRecipes>> {
        self.machine_recipes.lock().ok().and_then(|s| s.clone())
    }
    /// Some running mod reads capability `machine-recipes`.
    pub fn wants_machine_recipes(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.run.as_ref().is_some_and(|r| r.reads_machine_recipes))
    }
    /// Some running mod has a `frame` hook: the game refreshes the snapshot
    /// every frame for it.
    pub fn wants_frames(&self) -> bool {
        self.entries.iter().any(|e| {
            e.run
                .as_ref()
                .is_some_and(|r| r.hooks.client().is_some_and(|h| h.frame))
        })
    }
    /// Some running mod claimed items (capability `input`).
    pub fn claims_any(&self) -> bool {
        self.inputs().any(|i| !i.claimed.is_empty())
    }
    /// Some running mod claimed the item `id`.
    pub fn claims(&self, id: &str) -> bool {
        self.inputs().any(|i| i.claimed.iter().any(|c| c == id))
    }
    /// Some running mod registered a key.
    pub fn has_keys(&self) -> bool {
        self.inputs().any(|i| !i.keys.is_empty())
    }
    /// The mods' keys: (mod name, key name, label, key), for Controls.
    pub fn mod_keys(&self) -> impl Iterator<Item = (&str, &ModKey)> {
        self.entries
            .iter()
            .filter_map(|e| e.run.as_ref())
            .flat_map(|r| {
                let d = r.store.data();
                d.input.keys.iter().map(move |k| (d.name.as_str(), k))
            })
    }
    /// Mod `i`'s keys, as it registered them (with the player's choices).
    pub fn keys_of(&self, i: usize) -> &[ModKey] {
        self.entries
            .get(i)
            .and_then(|e| e.run.as_ref())
            .map_or(&[], |r| r.store.data().input.keys.as_slice())
    }
    /// The keys the player chose (`ui`'s key widget), by mod id: each
    /// running mod's `name → key`. They replace the defaults registered
    /// so far and later ones. Keys that aren't keys are skipped.
    pub fn set_bindings<'a>(&mut self, chosen: impl Fn(&str) -> Vec<(&'a str, &'a str)>) {
        for e in &mut self.entries {
            let (Some(m), Some(run)) = (&e.manifest, e.run.as_mut()) else {
                continue;
            };
            let input = &mut run.store.data_mut().input;
            input.bindings = chosen(&m.id)
                .into_iter()
                .filter(|(_, key)| key_ok(key))
                .map(|(n, k)| (n.to_owned(), k.to_owned()))
                .collect();
            for (name, key) in input.bindings.clone() {
                input.rebind(&name, key);
            }
        }
    }
    /// The player binds mod `i`'s key `name` to `key` (`key.keyboard.x`,
    /// `key.mouse.4`, `key.keyboard.unknown` for none). False when the mod
    /// has no such key or `key` isn't one.
    pub fn rebind(&mut self, i: usize, name: &str, key: &str) -> bool {
        let Some(run) = self.entries.get_mut(i).and_then(|e| e.run.as_mut()) else {
            return false;
        };
        let input = &mut run.store.data_mut().input;
        if !key_ok(key) || !input.keys.iter().any(|k| k.name == name) {
            return false;
        }
        input.bindings.retain(|(n, _)| n != name);
        input.bindings.push((name.to_owned(), key.to_owned()));
        input.rebind(name, key.to_owned());
        true
    }
    /// The use button went down with `item`: `use-start` to each mod that
    /// claimed it.
    pub fn use_start(&mut self, item: &HeldItem) {
        let event = InputEvent::UseStart(ItemUse {
            item: item.clone(),
            ticks: 0,
        });
        self.push_input(|i| i.claimed.contains(&item.id), &event);
    }
    /// The claimed use ended after `ticks`.
    pub fn use_stop(&mut self, item: &HeldItem, ticks: u32) {
        let event = InputEvent::UseStop(ItemUse {
            item: item.clone(),
            ticks,
        });
        self.push_input(|i| i.claimed.contains(&item.id), &event);
    }
    /// The key `key` (`key.keyboard.r`) went down or up: each mod with a
    /// key on it hears its own name for it.
    pub fn key(&mut self, key: &str, down: bool) {
        for run in self.entries.iter_mut().filter_map(|e| e.run.as_mut()) {
            let input = &mut run.store.data_mut().input;
            let names: Vec<String> = input
                .keys
                .iter()
                .filter(|k| k.key == key)
                .map(|k| k.name.clone())
                .collect();
            for name in names {
                let held = input.down.iter().position(|n| *n == name);
                match (down, held) {
                    (true, None) => input.down.push(name.clone()),
                    (false, Some(i)) => {
                        input.down.swap_remove(i);
                    }
                    // Already let go when a screen opened.
                    (false, None) => continue,
                    (true, Some(_)) => {}
                }
                input.push(if down {
                    InputEvent::KeyDown(name)
                } else {
                    InputEvent::KeyUp(name)
                });
            }
        }
    }
    /// A screen opened, or the game stopped playing: every mod key held
    /// lets go (`key-up`), so a key needs a new press afterwards.
    pub fn release_keys(&mut self) {
        for run in self.entries.iter_mut().filter_map(|e| e.run.as_mut()) {
            let input = &mut run.store.data_mut().input;
            for name in std::mem::take(&mut input.down) {
                input.push(InputEvent::KeyUp(name));
            }
        }
    }
    /// Some running mod holds the mouse wheel.
    pub fn wheel_claimed(&self) -> bool {
        self.inputs().any(|i| i.wheel)
    }
    /// Wheel steps while playing: `wheel` to the mods holding it; false
    /// when none does (the hotbar takes them).
    pub fn wheel(&mut self, steps: f32) -> bool {
        let event = InputEvent::Wheel(steps);
        let claimed = self.wheel_claimed();
        self.push_input(|i| i.wheel, &event);
        claimed
    }
    /// The strongest zoom a running mod asks for (capability `render`).
    pub fn zoom(&self) -> Option<ModZoom> {
        self.entries
            .iter()
            .filter_map(|e| e.run.as_ref()?.store.data().zoom)
            .max_by(|a, b| a.factor.total_cmp(&b.factor))
    }
    /// The minimap the first running mod shows (capability `render`).
    pub fn minimap(&self) -> Option<ModMinimap> {
        self.entries
            .iter()
            .find_map(|e| e.run.as_ref()?.store.data().minimap)
    }
    /// The far terrain the first running mod draws (capability `render`).
    pub fn far_terrain(&self) -> Option<ModFarTerrain> {
        self.entries
            .iter()
            .find_map(|e| e.run.as_ref()?.store.data().far_terrain)
    }
    fn inputs(&self) -> impl Iterator<Item = &InputState> {
        self.entries
            .iter()
            .filter_map(|e| Some(&e.run.as_ref()?.store.data().input))
    }
    fn push_input(&mut self, to: impl Fn(&InputState) -> bool, event: &InputEvent) {
        for run in self.entries.iter_mut().filter_map(|e| e.run.as_mut()) {
            let input = &mut run.store.data_mut().input;
            if to(input) {
                input.push(event.clone());
            }
        }
    }
    /// The running mods' HUD shapes (capability `render`), each mod's in
    /// its own order, mods in load order.
    pub fn hud_shapes(&self) -> impl Iterator<Item = &Shape> {
        self.entries
            .iter()
            .filter_map(|e| e.run.as_ref())
            .flat_map(|r| &r.store.data().shapes)
    }
    /// The screen mod `i` last pushed with `ui.open-screen`, while it is up.
    pub fn screen(&self, i: usize) -> Option<&UiScreen> {
        self.entries
            .get(i)?
            .run
            .as_ref()?
            .store
            .data()
            .ui
            .screen
            .as_ref()
    }
    /// The widgets `ui.set-settings` pushed: mod `i`'s page in
    /// Options > Mods. Empty while it has none.
    pub fn settings_widgets(&self, i: usize) -> &[UiWidget] {
        self.entries
            .get(i)
            .and_then(|e| e.run.as_ref())
            .map_or(&[], |r| &r.store.data().ui.settings)
    }
    /// A `list` widget's row was picked on a surface of mod `i`.
    pub fn ui_select(&mut self, i: usize, id: u32, row: u32) {
        self.ui_event(i, |ui, s| {
            ui.horizon_mods_ui_events().call_select(s, id, row)
        });
    }
    /// The player left mod `i`'s screen (Esc or Done).
    pub fn ui_closed(&mut self, i: usize) {
        self.ui_event(i, |ui, s| ui.horizon_mods_ui_events().call_closed(s));
    }
    /// The player opened mod `i`'s settings page.
    pub fn ui_settings_opened(&mut self, i: usize) {
        self.ui_event(i, |ui, s| {
            ui.horizon_mods_ui_events().call_settings_opened(s)
        });
    }
    /// A `ui-events` call on mod `i`; no export, no call (mods built
    /// before `ui` have none).
    fn ui_event(
        &mut self,
        i: usize,
        f: impl FnOnce(&UiHost, &mut Store<State>) -> wasmtime::Result<()>,
    ) {
        let has_ui = self
            .entries
            .get(i)
            .and_then(|e| e.run.as_ref())
            .is_some_and(|r| r.ui.is_some());
        if has_ui {
            self.call(i, FUEL_EVENT, |run| {
                f(run.ui.as_ref().expect("checked above"), &mut run.store)
            });
        }
    }
    /// What mods asked to show since the last call.
    pub fn drain(&mut self) -> Vec<Output> {
        std::mem::take(&mut self.out)
    }

    /// Restarts the mods whose file changed since they loaded (mod
    /// development: rebuild, and the game picks it up). True when any did.
    pub fn reload_changed(&mut self, reserved: impl Fn(&str) -> bool) -> bool {
        let mut any = false;
        for i in 0..self.entries.len() {
            let entry = &self.entries[i];
            let now = modified(&entry.path);
            let was_on = !matches!(entry.status, Status::Off | Status::Elsewhere);
            if now == entry.modified || !was_on {
                continue;
            }
            any = true;
            self.generation += 1;
            self.entries[i].modified = now;
            self.entries[i].run = None;
            let path = self.entries[i].path.clone();
            let started = Package::read(&path).and_then(|p| {
                self.entries[i].manifest = Some(p.manifest.clone());
                self.start(&path, &p, &reserved)
            });
            let entry = &mut self.entries[i];
            let id = entry.manifest.as_ref().map_or("?", |m| m.id.as_str());
            match started {
                Ok(run) => {
                    log::info!("Horizon: mod {id} reloaded");
                    entry.run = run;
                    entry.status = Status::Running;
                }
                Err(e) => {
                    log::warn!("Horizon: mod {id} did not reload: {e}");
                    entry.status = Status::Failed(e);
                }
            }
        }
        any
    }
}

/// A command's arguments are well formed: names given and distinct,
/// optional ones last, free text only last.
fn check_arguments(arguments: &[Argument]) -> Result<(), String> {
    for (i, a) in arguments.iter().enumerate() {
        if a.name.is_empty()
            || !a
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(format!("argument name '{}' isn't [A-Za-z0-9_]+", a.name));
        }
        if arguments[..i].iter().any(|b| b.name == a.name) {
            return Err(format!("two arguments are named {}", a.name));
        }
        if i > 0 && arguments[i - 1].optional && !a.optional {
            return Err(format!("{} is required after an optional argument", a.name));
        }
        if a.kind == ArgumentKind::Text && i + 1 < arguments.len() {
            return Err(format!(
                "{} takes the rest of the line, so it must be last",
                a.name
            ));
        }
    }
    Ok(())
}

fn modified(path: &Path) -> Option<SystemTime> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.is_dir() {
        // A folder mod changes when its component does.
        return std::fs::metadata(path.join(crate::manifest::WASM))
            .ok()?
            .modified()
            .ok();
    }
    meta.modified().ok()
}

/// A trap or error, as the player reads it.
fn reason(e: &wasmtime::Error) -> String {
    match e.downcast_ref::<Trap>() {
        Some(Trap::OutOfFuel) => "it took too long (an endless loop?)".into(),
        Some(Trap::UnreachableCodeReached) => "it crashed (a panic)".into(),
        Some(trap) => format!("it crashed: {trap}"),
        None => format!("{e:#}"),
    }
}

fn clip(text: String) -> String {
    clip_to(text, LINE_CHARS)
}
fn clip_to(text: String, chars: usize) -> String {
    match text.char_indices().nth(chars) {
        Some((at, _)) => text[..at].to_string(),
        None => text,
    }
}

/// Widgets on a `ui` screen or settings page, and rows on one `list`.
const WIDGETS: usize = 64;
const LIST_ROWS: usize = 512;

fn clip_widgets(widgets: Vec<UiWidget>) -> Vec<UiWidget> {
    widgets
        .into_iter()
        .take(WIDGETS)
        .map(|w| match w {
            UiWidget::Text(mut t) => {
                t.text = clip(t.text);
                UiWidget::Text(t)
            }
            UiWidget::Button(mut b) => {
                b.label = clip(b.label);
                UiWidget::Button(b)
            }
            UiWidget::Item(mut i) => {
                i.item = clip(i.item);
                i.name = clip(i.name);
                UiWidget::Item(i)
            }
            UiWidget::List(mut l) => {
                l.rows = l.rows.into_iter().take(LIST_ROWS).map(clip).collect();
                l.selected = l.selected.filter(|&s| (s as usize) < l.rows.len());
                UiWidget::List(l)
            }
        })
        .collect()
}

impl mod_log::Host for State {
    fn write(&mut self, level: mod_log::Level, message: String) {
        let level = match level {
            mod_log::Level::Debug => log::Level::Debug,
            mod_log::Level::Info => log::Level::Info,
            mod_log::Level::Warn => log::Level::Warn,
            mod_log::Level::Error => log::Level::Error,
        };
        log::log!(level, "[mod {}] {}", self.id, clip(message));
    }
}
impl game::Host for State {
    fn get_player(&mut self) -> Option<Player> {
        self.snapshot.player.clone()
    }
    fn get_clock(&mut self) -> Option<Clock> {
        self.snapshot.clock
    }
    fn raycast(&mut self, distance: f32) -> Option<Hit> {
        let (at, hit) = self.snapshot.view_hit.as_ref()?;
        // NaN compares false: no hit.
        (*at <= distance.min(RAYCAST_MAX) && !distance.is_nan()).then(|| hit.clone())
    }
}
impl browser::Host for State {
    fn set_items(&mut self, items: Vec<BrowserStack>) {
        self.browser.items = items
            .into_iter()
            .take(BROWSER_ITEMS)
            .map(|s| BrowserStack {
                item: clip(s.item),
                components: clip_to(s.components, 4096),
                ..s
            })
            .collect();
        self.browser.version = BROWSER_VERSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    fn set_recipes(
        &mut self,
        title: String,
        tabs: Vec<BrowserStack>,
        tab: u32,
        recipes: Vec<BrowserRecipe>,
    ) {
        let stack = |s: BrowserStack| BrowserStack {
            item: clip(s.item),
            components: clip_to(s.components, 4096),
            ..s
        };
        let cycle = |list: Vec<BrowserStack>| -> Vec<BrowserStack> {
            list.into_iter().take(SLOT_STACKS).map(stack).collect()
        };
        self.browser.recipes = (!recipes.is_empty()).then(|| {
            let recipes = recipes
                .into_iter()
                .take(BROWSER_RECIPES)
                .map(|r| BrowserRecipe {
                    width: r.width.clamp(1, 3),
                    height: r.height.clamp(1, 3),
                    slots: r.slots.into_iter().take(RECIPE_SLOTS).map(cycle).collect(),
                    results: cycle(r.results),
                    note: clip(r.note),
                    ..r
                })
                .collect();
            RecipePage {
                title: clip(title),
                tabs: tabs.into_iter().take(RECIPE_TABS).map(stack).collect(),
                tab,
                recipes,
            }
        });
        self.browser.version = BROWSER_VERSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    fn poll(&mut self) -> Vec<BrowserEvent> {
        self.browser.events.drain(..).collect()
    }
}
impl recipes::Host for State {
    fn items(&mut self) -> Vec<RecipeItem> {
        self.snapshot
            .recipes
            .as_ref()
            .map_or_else(Vec::new, |r| r.items.clone())
    }
    fn all(&mut self) -> Vec<Recipe> {
        self.snapshot
            .recipes
            .as_ref()
            .map_or_else(Vec::new, |r| r.recipes.clone())
    }
    fn known(&mut self) -> Vec<i32> {
        self.snapshot.known_recipes.to_vec()
    }
    fn place(&mut self, book_id: i32, all: bool) -> Result<(), String> {
        self.recipe_action()?;
        if !self.snapshot.placing {
            return Err("no screen that takes recipes is open".into());
        }
        if !self.snapshot.known_recipes.contains(&book_id) {
            return Err(format!(
                "recipe {book_id} is not in the player's recipe book"
            ));
        }
        self.out.push(Output::PlaceRecipe { book_id, all });
        Ok(())
    }
    fn give(&mut self, stack: RecipeStack) -> Result<(), String> {
        self.recipe_action()?;
        if !self.snapshot.creative {
            return Err("only in creative mode".into());
        }
        let item = self
            .snapshot
            .recipes
            .as_ref()
            .and_then(|r| r.items.get(stack.item as usize))
            .ok_or_else(|| format!("no item {}", stack.item))?
            .id
            .clone();
        self.out.push(Output::Give {
            item,
            count: stack.count.clamp(1, 99),
            components: clip_to(stack.components, 4096),
        });
        Ok(())
    }
}
impl State {
    /// `recipes.place` and `give` take at most [`RECIPE_ACTIONS`] a call.
    fn recipe_action(&self) -> Result<(), String> {
        let done = self
            .out
            .iter()
            .filter(|o| matches!(o, Output::PlaceRecipe { .. } | Output::Give { .. }))
            .count();
        if done < RECIPE_ACTIONS {
            Ok(())
        } else {
            Err(format!("at most {RECIPE_ACTIONS} places and gives a call"))
        }
    }
}
impl State {
    /// Queues a chat line unless the call already sent [`CHAT_PER_CALL`].
    fn say(&mut self, line: impl FnOnce(String) -> Output) -> bool {
        let shown = self
            .out
            .iter()
            .filter(|o| {
                matches!(
                    o,
                    Output::Chat { .. } | Output::Tell { .. } | Output::Broadcast { .. }
                )
            })
            .count();
        if shown < CHAT_PER_CALL {
            let name = self.name.clone();
            self.out.push(line(name));
        }
        shown < CHAT_PER_CALL
    }
}
impl chat::Host for State {
    fn show(&mut self, text: String) {
        self.say(|name| Output::Chat {
            name,
            text: clip(text),
        });
    }
}
impl mod_world::Host for State {
    fn get_block(&mut self, dimension: String, x: i32, y: i32, z: i32) -> Option<String> {
        let view = self.world.lock().ok()?;
        view.as_ref()?.block(&dimension, x, y, z)
    }
    fn get_biome(&mut self, dimension: String, x: i32, y: i32, z: i32) -> Option<String> {
        let view = self.world.lock().ok()?;
        view.as_ref()?.biome(&dimension, x, y, z)
    }
    fn set_blocks(&mut self, dimension: String, blocks: Vec<BlockChange>) -> Result<(), String> {
        if blocks.len() > BLOCKS_PER_CALL {
            return Err(format!(
                "{} blocks: at most {BLOCKS_PER_CALL} per call",
                blocks.len()
            ));
        }
        if let Some(view) = self.world.lock().ok().as_ref().and_then(|v| v.as_ref()) {
            if !view.has_dimension(&dimension) {
                return Err(format!("no dimension {dimension}"));
            }
            for b in &blocks {
                view.check_state(&b.state)?;
            }
        }
        if !blocks.is_empty() {
            self.out.push(Output::SetBlocks {
                name: self.name.clone(),
                dimension,
                blocks,
            });
        }
        Ok(())
    }
}
impl block_entities::Host for State {
    fn get(&mut self, dimension: String, x: i32, y: i32, z: i32) -> Option<Vec<u8>> {
        let slot = self.entities.lock().ok()?;
        let data = slot.as_ref()?.get(&self.id, &(dimension, [x, y, z]))?;
        Some(data.to_vec())
    }
    fn set(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        data: Vec<u8>,
    ) -> Result<(), String> {
        let mut slot = self
            .entities
            .lock()
            .map_err(|_| "block entities are unavailable".to_string())?;
        let store = slot
            .as_mut()
            .ok_or("block entities are kept only during server hooks")?;
        store.set(&self.id, &(dimension, [x, y, z]), data)
    }
}
/// A menu's slots, checked against its rows.
fn check_menu_items(rows: u32, items: &[MenuItem]) -> Result<(), String> {
    if let Some(i) = items.iter().find(|i| i.slot >= rows * 9) {
        return Err(format!(
            "slot {} past the menu's {} slots",
            i.slot,
            rows * 9
        ));
    }
    if let Some(i) = items.iter().find(|i| i.count == 0 || i.count > 99) {
        return Err(format!("count {} in slot {}: 1 to 99", i.count, i.slot));
    }
    Ok(())
}
impl displays::Host for State {
    fn show(
        &mut self,
        name: String,
        dimension: String,
        x: f64,
        y: f64,
        z: f64,
        shape: DisplayShape,
        transform: Transform,
        interpolation: u32,
    ) -> Result<(), String> {
        if let Some(view) = self.world.lock().ok().as_ref().and_then(|v| v.as_ref()) {
            if !view.has_dimension(&dimension) {
                return Err(format!("no dimension {dimension}"));
            }
            if let DisplayShape::Block(state) = &shape {
                view.check_state(state)?;
            }
        }
        if ![x, y, z].iter().all(|v| v.is_finite()) {
            return Err("a position must be finite".into());
        }
        self.out.push(Output::ShowDisplay {
            mod_id: self.id.clone(),
            name,
            dimension,
            pos: [x, y, z],
            shape,
            transform,
            interpolation: interpolation.min(1200),
        });
        Ok(())
    }
    fn hide(&mut self, name: String) {
        self.out.push(Output::HideDisplay {
            mod_id: self.id.clone(),
            name,
        });
    }
}
/// A mob tag: `[a-z0-9_./-]{1,64}`.
fn entity_tag_ok(tag: &str) -> bool {
    horizon_protocol_name_ok(tag)
}
impl State {
    /// The lent mobs, for capability `entities`.
    fn with_entities<R>(&self, f: impl FnOnce(&mut EntityState, &str) -> R) -> Result<R, String> {
        let mut slot = self
            .mod_entities
            .lock()
            .map_err(|_| "mobs are unavailable".to_string())?;
        let state = slot
            .as_mut()
            .ok_or("mobs are kept only during server hooks")?;
        Ok(f(state, &self.id))
    }
    /// The mod's own mob `id` (error otherwise).
    fn own_entity(&self, id: u32) -> Result<EntityInfo, String> {
        self.with_entities(|s, me| {
            s.entities
                .iter()
                .find(|e| e.id == id && e.owner == me)
                .cloned()
        })?
        .ok_or_else(|| format!("no mob {id} of this mod"))
    }
}
impl entities::Host for State {
    fn spawn(
        &mut self,
        dimension: String,
        x: f64,
        y: f64,
        z: f64,
        options: EntitySpawn,
    ) -> Result<u32, String> {
        if ![x, y, z].iter().all(|v| v.is_finite()) {
            return Err("a position must be finite".into());
        }
        if !entity_tag_ok(&options.tag) {
            return Err(format!("tag {:?} is not [a-z0-9_./-]{{1,64}}", options.tag));
        }
        if options
            .name
            .as_ref()
            .is_some_and(|n| n.chars().count() > 64)
        {
            return Err("a name is up to 64 characters".into());
        }
        if options.health.is_some_and(|h| !h.is_finite() || h <= 0.0) {
            return Err("health must be above 0".into());
        }
        if self.entity_spawns >= SPAWNS_PER_CALL {
            return Err(format!("at most {SPAWNS_PER_CALL} spawns in one call"));
        }
        let kind = if options.kind.contains(':') {
            options.kind.clone()
        } else {
            format!("minecraft:{}", options.kind)
        };
        let (id, info) = self.with_entities(|s, me| {
            if !s.can_spawn {
                return Err("mobs can't be spawned from this hook".to_string());
            }
            if dimension != s.dimension {
                return Err(format!(
                    "the server is simulating {}, not {dimension}",
                    s.dimension
                ));
            }
            if !s.kinds.contains(&kind) {
                return Err(format!("no mob kind {kind}"));
            }
            if s.entities.iter().filter(|e| e.owner == me).count() >= MAX_ENTITIES {
                return Err(format!("the mod has {MAX_ENTITIES} mobs already"));
            }
            let id = s.next_id.max(1);
            s.next_id = id + 1;
            let health = options.health.unwrap_or(20.0);
            let info = EntityInfo {
                id,
                owner: me.to_owned(),
                kind: kind.clone(),
                tag: options.tag.clone(),
                dimension: dimension.clone(),
                pos: [x, y, z],
                yaw: 0.0,
                health,
                max_health: health,
                dying: false,
            };
            s.entities.push(info.clone());
            Ok((id, info))
        })??;
        let _ = info;
        self.entity_spawns += 1;
        self.out.push(Output::SpawnEntity {
            mod_id: self.id.clone(),
            id,
            pos: [x, y, z],
            options: EntitySpawn { kind, ..options },
        });
        Ok(id)
    }
    fn get(&mut self, id: u32) -> Option<entities::Entity> {
        self.own_entity(id).ok().map(|e| e.to_wit())
    }
    fn list(&mut self) -> Vec<entities::Entity> {
        self.with_entities(|s, me| {
            s.entities
                .iter()
                .filter(|e| e.owner == me)
                .map(EntityInfo::to_wit)
                .collect()
        })
        .unwrap_or_default()
    }
    fn teleport(&mut self, id: u32, x: f64, y: f64, z: f64) -> Result<(), String> {
        self.own_entity(id)?;
        if ![x, y, z].iter().all(|v| v.is_finite()) {
            return Err("a position must be finite".into());
        }
        // The mod reads the new place at once.
        self.with_entities(|s, _| {
            if let Some(e) = s.entities.iter_mut().find(|e| e.id == id) {
                e.pos = [x, y, z];
            }
        })?;
        self.out.push(Output::TeleportEntity {
            mod_id: self.id.clone(),
            id,
            pos: [x, y, z],
        });
        Ok(())
    }
    fn push(&mut self, id: u32, x: f64, y: f64, z: f64) -> Result<(), String> {
        self.own_entity(id)?;
        if ![x, y, z].iter().all(|v| v.is_finite()) {
            return Err("a velocity must be finite".into());
        }
        self.out.push(Output::PushEntity {
            mod_id: self.id.clone(),
            id,
            velocity: [x, y, z],
        });
        Ok(())
    }
    fn set_health(&mut self, id: u32, health: f32) -> Result<(), String> {
        let e = self.own_entity(id)?;
        if !health.is_finite() || health < 0.0 {
            return Err("health must be 0 or more".into());
        }
        let health = health.min(e.max_health);
        self.with_entities(|s, _| {
            if let Some(e) = s.entities.iter_mut().find(|e| e.id == id) {
                e.health = health;
            }
        })?;
        self.out.push(Output::SetEntityHealth {
            mod_id: self.id.clone(),
            id,
            health,
        });
        Ok(())
    }
    fn remove(&mut self, id: u32) {
        if self.own_entity(id).is_ok() {
            self.out.push(Output::RemoveEntity {
                mod_id: self.id.clone(),
                id,
            });
        }
    }
}
impl State {
    /// One more message of `data` bytes in this call, within its caps.
    fn net_budget(&mut self, channel: &str, data: &[u8]) -> Result<(), String> {
        if !horizon_protocol_name_ok(channel) {
            return Err(format!("channel {channel:?} is not [a-z0-9_./-]{{1,64}}"));
        }
        if data.len() > NET_MAX_DATA {
            return Err(format!("{} bytes: at most {NET_MAX_DATA}", data.len()));
        }
        let (n, bytes) = self.net_sent;
        if n >= NET_MESSAGES_PER_CALL || bytes + data.len() > NET_BYTES_PER_CALL {
            return Err("too many messages in one call".into());
        }
        self.net_sent = (n + 1, bytes + data.len());
        Ok(())
    }
}
/// Bytes of a message at most (`horizon_protocol::mod_net::MAX_DATA`) and the
/// rule for its channel name, kept here so the runtime needs no protocol crate.
const NET_MAX_DATA: usize = 30_000;
fn horizon_protocol_name_ok(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'/' | b'-')
        })
}
impl network::Host for State {
    fn send_to(&mut self, player: String, channel: String, data: Vec<u8>) -> Result<(), String> {
        if !self.server {
            return Err("a client mod sends to its server half with `send`".into());
        }
        if !self.snapshot.players.iter().any(|p| p.id == player) {
            return Err(format!("no player {player} on the server"));
        }
        self.net_budget(&channel, &data)?;
        self.out.push(Output::NetToClient {
            mod_id: self.id.clone(),
            player,
            channel,
            data,
        });
        Ok(())
    }
    fn send(&mut self, channel: String, data: Vec<u8>) -> Result<(), String> {
        if self.server {
            return Err("a server mod sends to players with `send-to`".into());
        }
        self.net_budget(&channel, &data)?;
        self.out.push(Output::NetToServer {
            mod_id: self.id.clone(),
            channel,
            data,
        });
        Ok(())
    }
}
impl menus::Host for State {
    fn open(
        &mut self,
        player: String,
        menu: String,
        title: String,
        rows: u32,
        items: Vec<MenuItem>,
    ) -> Result<(), String> {
        if !(1..=MENU_ROWS).contains(&rows) {
            return Err(format!("{rows} rows: 1 to {MENU_ROWS}"));
        }
        check_menu_items(rows, &items)?;
        if !self.snapshot.players.iter().any(|p| p.id == player) {
            return Err(format!("no player {player} on the server"));
        }
        self.out.push(Output::OpenMenu {
            mod_id: self.id.clone(),
            player,
            menu,
            title,
            rows,
            items,
            inventory: None,
        });
        Ok(())
    }
    fn open_inventory(
        &mut self,
        player: String,
        menu: String,
        title: String,
        rows: u32,
        items: Vec<MenuItem>,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        slots: Vec<MenuSlot>,
    ) -> Result<(), String> {
        if let Some(s) = slots.iter().find(|s| s.slot >= rows * 9) {
            return Err(format!(
                "slot {} past the menu's {} slots",
                s.slot,
                rows * 9
            ));
        }
        let key = (dimension.clone(), [x, y, z]);
        let size = self
            .with_store(|s, id| match s.items.get(&key) {
                Some(inv) if s.get(id, &key).is_some() => Ok(inv.len()),
                _ => Err(format!(
                    "no inventory of this mod at {x} {y} {z} in {dimension}"
                )),
            })
            .map_err(|e| e.to_string())??;
        if let Some(s) = slots.iter().find(|s| s.inventory_slot as usize >= size) {
            return Err(format!("inventory slot {} of {size}", s.inventory_slot));
        }
        menus::Host::open(self, player, menu, title, rows, items)?;
        if let Some(Output::OpenMenu { inventory, .. }) = self.out.last_mut() {
            *inventory = Some(MenuInventory {
                dimension,
                pos: [x, y, z],
                slots,
            });
        }
        Ok(())
    }
    fn set_items(
        &mut self,
        player: String,
        menu: String,
        items: Vec<MenuItem>,
    ) -> Result<(), String> {
        check_menu_items(MENU_ROWS, &items)?;
        self.out.push(Output::SetMenuItems {
            mod_id: self.id.clone(),
            player,
            menu,
            items,
        });
        Ok(())
    }
    fn close(&mut self, player: String) {
        self.out.push(Output::CloseMenu {
            mod_id: self.id.clone(),
            player,
        });
    }
}
impl State {
    /// The lent block entity store, for capability `energy`.
    fn with_store<R>(
        &mut self,
        f: impl FnOnce(&mut crate::BlockEntities, &str) -> R,
    ) -> Result<R, String> {
        let mut slot = self
            .entities
            .lock()
            .map_err(|_| "block entities are unavailable".to_string())?;
        let store = slot
            .as_mut()
            .ok_or("the server's stores are kept only during server hooks")?;
        Ok(f(store, &self.id))
    }
}
/// A capability `items` stack as the store keeps it.
fn to_store(s: mod_items::Stack) -> crate::items::Stack {
    crate::items::Stack {
        item: s.item,
        count: s.count,
        components: s.components,
    }
}
fn from_store(s: &crate::items::Stack) -> mod_items::Stack {
    mod_items::Stack {
        item: s.item.clone(),
        count: s.count,
        components: s.components.clone(),
    }
}
impl State {
    /// An item's stack size, from the lent world; 64 with none lent (the
    /// registry's trial run).
    fn max_stack(&self, item: &str) -> Option<u32> {
        match self.world.lock().ok().as_ref().and_then(|v| v.as_ref()) {
            Some(view) => view.max_stack(item),
            None => Some(64),
        }
    }
}
impl mod_items::Host for State {
    fn set_slots(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        slots: u32,
    ) -> Result<(), String> {
        let key = (dimension, [x, y, z]);
        let lost = self.with_store(|s, id| {
            if s.get(id, &key).is_none() {
                return Err(format!(
                    "no block entity of this mod at {x} {y} {z} in {}",
                    key.0
                ));
            }
            s.items.set_slots(id, key.clone(), slots as usize)
        })??;
        if !lost.is_empty() {
            self.out.push(Output::DropItems {
                dimension: key.0,
                pos: key.1,
                stacks: lost,
            });
        }
        Ok(())
    }
    fn get(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
    ) -> Option<Vec<Option<mod_items::Stack>>> {
        let key = (dimension, [x, y, z]);
        self.with_store(|s, _| {
            s.items
                .get(&key)
                .map(|slots| slots.iter().map(|s| s.as_ref().map(from_store)).collect())
        })
        .ok()?
    }
    fn set(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        slot: u32,
        stack: Option<mod_items::Stack>,
    ) -> Result<(), String> {
        if let Some(st) = &stack {
            let max = self
                .max_stack(&st.item)
                .ok_or_else(|| format!("no item {}", st.item))?;
            if st.count > max {
                return Err(format!(
                    "{} {}: at most {max} in a stack",
                    st.count, st.item
                ));
            }
        }
        let key = (dimension, [x, y, z]);
        self.with_store(|s, id| s.items.set(id, &key, slot as usize, stack.map(to_store)))?
    }
    fn insert(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        stack: mod_items::Stack,
        simulate: bool,
    ) -> u32 {
        let Some(max) = self.max_stack(&stack.item) else {
            return 0;
        };
        let key = (dimension, [x, y, z]);
        let stack = to_store(stack);
        self.with_store(|s, _| s.items.insert(&key, &stack, max, simulate))
            .unwrap_or(0)
    }
    fn extract(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        slot: u32,
        count: u32,
        simulate: bool,
    ) -> Option<mod_items::Stack> {
        let key = (dimension, [x, y, z]);
        self.with_store(|s, _| s.items.extract(&key, slot as usize, count, simulate))
            .ok()?
            .map(|s| from_store(&s))
    }
}
fn sides_to_wit(s: crate::redstone::Sides) -> redstone::Sides {
    redstone::Sides {
        down: s[0],
        up: s[1],
        north: s[2],
        south: s[3],
        west: s[4],
        east: s[5],
    }
}
fn sides_from_wit(s: &redstone::Sides) -> crate::redstone::Sides {
    [s.down, s.up, s.north, s.south, s.west, s.east]
}
/// A sound or particle name: `namespace:path` or a bare path (the game's
/// `minecraft:`), in the characters an identifier has.
fn effect_name_ok(name: &str) -> bool {
    let plain = |t: &str| {
        !t.is_empty()
            && t.len() <= 96
            && t.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-/".contains(&b))
    };
    match name.split_once(':') {
        Some((ns, path)) => plain(ns) && plain(path),
        None => plain(name),
    }
}
/// `SoundSource` index of a category name, in vanilla's order.
fn sound_category(name: &str) -> Option<i32> {
    Some(match name {
        "master" => 0,
        "music" => 1,
        "record" => 2,
        "weather" => 3,
        "block" => 4,
        "hostile" => 5,
        "neutral" => 6,
        "player" => 7,
        "ambient" => 8,
        "voice" => 9,
        _ => return None,
    })
}
impl State {
    /// Counts one effect of this call: an error past the limit.
    fn count_effect(&mut self) -> Result<(), String> {
        if self.effects_sent >= EFFECTS_PER_CALL {
            return Err(format!("at most {EFFECTS_PER_CALL} effects a call"));
        }
        self.effects_sent += 1;
        Ok(())
    }
}
impl State {
    /// Writes the config file (capability `config`).
    fn write_config(&self, text: &str) -> Result<(), String> {
        if let Some(dir) = self.config_path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&self.config_path, text).map_err(|e| e.to_string())
    }
    /// What a reading of the file asks of the game: the file put right, and
    /// what was wrong in it said in the log.
    fn apply_config_reload(&mut self, reload: &crate::config::Reload) {
        for p in &reload.problems {
            log::warn!("Horizon: mod {} config: {p}", self.id);
        }
        if let Some(text) = &reload.rewrite
            && let Err(e) = self.write_config(text)
        {
            log::warn!("Horizon: mod {} config not written: {e}", self.id);
        }
    }
}
fn config_spec_from_wit(s: mod_config::Spec) -> crate::config::Spec {
    use crate::config::Spec;
    match s {
        mod_config::Spec::Flag(b) => Spec::Boolean(b),
        mod_config::Spec::Integer(i) => Spec::Integer {
            default: i.initial,
            min: i.min,
            max: i.max,
        },
        mod_config::Spec::Number(n) => Spec::Number {
            default: n.initial,
            min: n.min,
            max: n.max,
        },
        mod_config::Spec::Text(t) => Spec::Text(t),
        mod_config::Spec::Choice(c) => Spec::Choice {
            default: c.initial,
            options: c.options,
        },
    }
}
fn config_value_from_wit(v: mod_config::Value) -> crate::config::Value {
    use crate::config::Value;
    match v {
        mod_config::Value::Flag(b) => Value::Boolean(b),
        mod_config::Value::Integer(n) => Value::Integer(n),
        mod_config::Value::Number(n) => Value::Number(n),
        mod_config::Value::Text(t) => Value::Text(t),
    }
}
fn config_value_to_wit(v: &crate::config::Value) -> mod_config::Value {
    use crate::config::Value;
    match v {
        Value::Boolean(b) => mod_config::Value::Flag(*b),
        Value::Integer(n) => mod_config::Value::Integer(*n),
        Value::Number(n) => mod_config::Value::Number(*n),
        Value::Text(t) => mod_config::Value::Text(t.clone()),
    }
}
impl mod_config::Host for State {
    fn declare(&mut self, entries: Vec<mod_config::Entry>) -> Result<(), String> {
        let entries = entries
            .into_iter()
            .map(|e| crate::config::Entry {
                key: e.key,
                description: e.description,
                spec: config_spec_from_wit(e.spec),
            })
            .collect();
        let text = std::fs::read_to_string(&self.config_path).ok();
        let reload = self.config.declare(entries, text.as_deref())?;
        self.apply_config_reload(&reload);
        Ok(())
    }
    fn get(&mut self, key: String) -> Option<mod_config::Value> {
        self.config.get(&key).map(config_value_to_wit)
    }
    fn set(&mut self, key: String, value: mod_config::Value) -> Result<(), String> {
        if self.config_writes >= CONFIG_WRITES_PER_CALL {
            return Err(format!(
                "at most {CONFIG_WRITES_PER_CALL} config writes a call"
            ));
        }
        let text = self.config.set(&key, &config_value_from_wit(value))?;
        self.config_writes += 1;
        self.write_config(&text)
    }
}
impl effects::Host for State {
    fn spawn_particle(&mut self, dimension: String, p: effects::Particle) -> Result<(), String> {
        if !effect_name_ok(&p.name) {
            return Err(format!(
                "{:?} is not a particle name (namespace:path)",
                p.name
            ));
        }
        if ![p.x, p.y, p.z].iter().all(|v| v.is_finite()) {
            return Err("a position must be finite".into());
        }
        let spread = [p.spread_x, p.spread_y, p.spread_z];
        if spread
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=64.0).contains(v))
        {
            return Err("a spread is 0 to 64".into());
        }
        if !p.speed.is_finite() || !(0.0..=10.0).contains(&p.speed) {
            return Err("a speed is 0 to 10".into());
        }
        if p.count > MAX_PARTICLES {
            return Err(format!("at most {MAX_PARTICLES} particles a call"));
        }
        if p.color.is_some_and(|c| c > 0xFF_FFFF) {
            return Err("a colour is 0xRRGGBB".into());
        }
        if p.scale
            .is_some_and(|s| !s.is_finite() || !(0.01..=4.0).contains(&s))
        {
            return Err("a scale is 0.01 to 4".into());
        }
        if p.block.as_ref().is_some_and(|b| b.len() > 200) {
            return Err("a block state is at most 200 characters".into());
        }
        self.count_effect()?;
        self.out.push(Output::Particle {
            dimension,
            particle: EffectParticle {
                name: p.name,
                pos: [p.x, p.y, p.z],
                spread,
                speed: p.speed,
                count: p.count,
                color: p.color,
                scale: p.scale,
                block: p.block,
                always_show: p.always_show,
            },
        });
        Ok(())
    }
    fn play_sound(
        &mut self,
        dimension: String,
        sound: String,
        x: f64,
        y: f64,
        z: f64,
        volume: f32,
        pitch: f32,
        category: String,
    ) -> Result<(), String> {
        if !effect_name_ok(&sound) {
            return Err(format!("{sound:?} is not a sound name (namespace:path)"));
        }
        let category = sound_category(&category)
            .ok_or_else(|| format!("{category:?} is not a sound category"))?;
        if ![x, y, z].iter().all(|v| v.is_finite()) || !volume.is_finite() || !pitch.is_finite() {
            return Err("a position, volume and pitch must be finite".into());
        }
        self.count_effect()?;
        self.out.push(Output::Sound {
            dimension,
            name: sound,
            pos: [x, y, z],
            volume: volume.clamp(0.0, 10.0),
            pitch: pitch.clamp(0.5, 2.0),
            category,
        });
        Ok(())
    }
    fn play_sound_to(
        &mut self,
        player: String,
        sound: String,
        volume: f32,
        pitch: f32,
        category: String,
    ) -> Result<(), String> {
        if !effect_name_ok(&sound) {
            return Err(format!("{sound:?} is not a sound name (namespace:path)"));
        }
        let category = sound_category(&category)
            .ok_or_else(|| format!("{category:?} is not a sound category"))?;
        if !volume.is_finite() || !pitch.is_finite() {
            return Err("a volume and pitch must be finite".into());
        }
        self.count_effect()?;
        self.out.push(Output::SoundTo {
            player,
            name: sound,
            volume: volume.clamp(0.0, 10.0),
            pitch: pitch.clamp(0.5, 2.0),
            category,
        });
        Ok(())
    }
}
impl redstone::Host for State {
    fn get_input(&mut self, dimension: String, x: i32, y: i32, z: i32) -> Option<redstone::Sides> {
        let key = (dimension, [x, y, z]);
        self.with_store(|s, id| {
            s.get(id, &key)?;
            s.redstone.inputs.get(&key).copied()
        })
        .ok()?
        .map(sides_to_wit)
    }
    fn set_output(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        output: redstone::Output,
    ) -> Result<(), String> {
        let key = (dimension.clone(), [x, y, z]);
        let output = crate::redstone::Output {
            power: sides_from_wit(&output.power),
            strong: output.strong,
        };
        let changed = self.with_store(|s, id| {
            if s.get(id, &key).is_none() {
                return Err(format!(
                    "no block entity of this mod at {x} {y} {z} in {dimension}"
                ));
            }
            s.redstone.set_output(id, key, output)
        })??;
        if changed {
            self.out.push(Output::RedstoneOutput {
                dimension,
                pos: [x, y, z],
                power: output.power,
                strong: output.strong && !output.is_off(),
            });
        }
        Ok(())
    }
    fn get_output(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
    ) -> Option<redstone::Output> {
        let key = (dimension, [x, y, z]);
        let o = self.with_store(|s, _| s.redstone.output(&key)).ok()??;
        Some(redstone::Output {
            power: sides_to_wit(o.power),
            strong: o.strong,
        })
    }
}
impl fluids::Host for State {
    fn set_tanks(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        tanks: Vec<fluids::TankSpec>,
    ) -> Result<(), String> {
        let key = (dimension, [x, y, z]);
        let specs: Vec<crate::fluids::Spec> = tanks
            .iter()
            .map(|t| crate::fluids::Spec {
                capacity: t.capacity,
                max_receive: t.max_receive,
                max_extract: t.max_extract,
            })
            .collect();
        self.with_store(|s, id| {
            if s.get(id, &key).is_none() {
                return Err(format!(
                    "no block entity of this mod at {x} {y} {z} in {}",
                    key.0
                ));
            }
            s.fluids.set_tanks(id, key, &specs)
        })?
    }
    fn get(&mut self, dimension: String, x: i32, y: i32, z: i32) -> Option<Vec<fluids::Tank>> {
        let tanks = self
            .with_store(|s, _| s.fluids.get(&(dimension, [x, y, z])))
            .ok()??;
        Some(
            tanks
                .into_iter()
                .map(|t| fluids::Tank {
                    fluid: t.fluid,
                    amount: t.amount,
                    capacity: t.spec.capacity,
                    max_receive: t.spec.max_receive,
                    max_extract: t.spec.max_extract,
                })
                .collect(),
        )
    }
    fn set(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        tank: u32,
        fluid: Option<String>,
        amount: u64,
    ) -> Result<(), String> {
        let key = (dimension, [x, y, z]);
        self.with_store(|s, id| {
            s.fluids
                .set(id, &key, tank as usize, fluid.as_deref(), amount)
        })?
    }
    fn insert(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        fluid: String,
        amount: u64,
        simulate: bool,
    ) -> u64 {
        let key = (dimension, [x, y, z]);
        self.with_store(|s, _| s.fluids.insert(&key, &fluid, amount, simulate))
            .unwrap_or(0)
    }
    fn extract(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        tank: u32,
        amount: u64,
        simulate: bool,
    ) -> Option<fluids::FluidStack> {
        let key = (dimension, [x, y, z]);
        let (fluid, amount) = self
            .with_store(|s, _| s.fluids.extract(&key, tank as usize, amount, simulate))
            .ok()??;
        Some(fluids::FluidStack { fluid, amount })
    }
    fn push(&mut self, dimension: String, x: i32, y: i32, z: i32, tank: u32, amount: u64) -> u64 {
        let key = (dimension, [x, y, z]);
        self.with_store(|s, id| s.fluids.push(id, &key, tank as usize, amount))
            .ok()
            .and_then(Result::ok)
            .unwrap_or(0)
    }
    fn fluid_of_bucket(&mut self, item: String) -> Option<String> {
        crate::fluids::fluid_of_bucket(&item).map(str::to_owned)
    }
    fn bucket_of_fluid(&mut self, fluid: String) -> Option<String> {
        crate::fluids::bucket_of_fluid(&fluid).map(str::to_owned)
    }
}
impl energy::Host for State {
    fn get(&mut self, dimension: String, x: i32, y: i32, z: i32) -> Option<energy::Storage> {
        let s = self
            .with_store(|s, _| s.energy.get(&(dimension, [x, y, z])))
            .ok()??;
        Some(energy::Storage {
            stored: s.stored,
            capacity: s.capacity,
            max_receive: s.max_receive,
            max_extract: s.max_extract,
        })
    }
    fn set_storage(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        capacity: u64,
        max_receive: u64,
        max_extract: u64,
    ) -> Result<(), String> {
        let key = (dimension, [x, y, z]);
        self.with_store(|s, id| {
            if s.get(id, &key).is_none() {
                return Err(format!(
                    "no block entity of this mod at {x} {y} {z} in {}",
                    key.0
                ));
            }
            s.energy
                .set_storage(id, key, capacity, max_receive, max_extract)
        })?
    }
    fn set_stored(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        stored: u64,
    ) -> Result<(), String> {
        self.with_store(|s, id| s.energy.set_stored(id, &(dimension, [x, y, z]), stored))?
    }
    fn insert(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        amount: u64,
        simulate: bool,
    ) -> u64 {
        let key = (dimension, [x, y, z]);
        self.with_store(|s, _| s.energy.insert(&key, amount, simulate))
            .unwrap_or(0)
    }
    fn extract(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        amount: u64,
        simulate: bool,
    ) -> u64 {
        let key = (dimension, [x, y, z]);
        self.with_store(|s, _| s.energy.extract(&key, amount, simulate))
            .unwrap_or(0)
    }
    fn push(&mut self, dimension: String, x: i32, y: i32, z: i32, amount: u64) -> u64 {
        let key = (dimension, [x, y, z]);
        self.with_store(|s, id| s.energy.push(id, &key, amount))
            .ok()
            .and_then(Result::ok)
            .unwrap_or(0)
    }
}
/// `machine-recipes` kinds one `all` or `by-input` call may ask for; the
/// rest are ignored (a kind not asked for simply finds nothing).
const MACHINE_RECIPE_KINDS: usize = 16;

impl machine_recipes::Host for State {
    fn all(&mut self, mut kinds: Vec<String>) -> Vec<machine_recipes::MachineRecipe> {
        kinds.truncate(MACHINE_RECIPE_KINDS);
        self.recipe_table()
            .map_or_else(Vec::new, |t| t.of_mod(&self.id, &kinds))
            .into_iter()
            .map(wit_recipe)
            .collect()
    }
    fn by_input(
        &mut self,
        stack: machine_recipes::Stack,
        mut kinds: Vec<String>,
    ) -> Vec<machine_recipes::MachineRecipe> {
        kinds.truncate(MACHINE_RECIPE_KINDS);
        self.recipe_table()
            .map_or_else(Vec::new, |t| t.by_input(&self.id, &stack.item, &kinds))
            .into_iter()
            .map(wit_recipe)
            .collect()
    }
}
impl State {
    /// The machine-recipes table this run loaded, if the game gave it one.
    fn recipe_table(&self) -> Option<Arc<crate::machine_recipes::MachineRecipes>> {
        self.machine_recipes.lock().ok().and_then(|s| s.clone())
    }
}

/// A stored machine recipe as the component reads it.
fn wit_recipe(r: crate::machine_recipes::MachineRecipe) -> machine_recipes::MachineRecipe {
    let slots = |slots: Vec<Vec<crate::machine_recipes::Stack>>| {
        slots
            .into_iter()
            .map(|s| {
                s.into_iter()
                    .map(|s| machine_recipes::Stack {
                        item: s.item,
                        count: s.count,
                    })
                    .collect()
            })
            .collect()
    };
    machine_recipes::MachineRecipe {
        id: r.id,
        kind: r.kind,
        inputs: slots(r.inputs),
        results: slots(r.results),
        data: r.data,
    }
}

impl multiblocks::Host for State {
    fn all(&mut self) -> Vec<Structure> {
        self.template_table()
            .map_or_else(Vec::new, |t| t.of_mod(&self.id).to_vec())
            .into_iter()
            .map(wit_structure)
            .collect()
    }
    fn check(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        quarter: u32,
        id: String,
    ) -> Result<Option<Vec<Mismatch>>, String> {
        let no_such = || format!("no structure {id} of this mod");
        let table = self.template_table();
        let structure = table
            .as_deref()
            .and_then(|t| t.of_mod(&self.id).iter().find(|s| s.id == id))
            .ok_or_else(no_such)?;
        // A read of the server's world: it answers during the server's hooks.
        let world = self
            .world
            .lock()
            .map_err(|_| "the world is unavailable".to_string())?;
        let view = world
            .as_ref()
            .ok_or("the world is kept only during server hooks")?;
        crate::multiblocks::check_cells(structure, [x, y, z], quarter, &|bx, by, bz| {
            view.block(&dimension, bx, by, bz)
        })
        .map(|mismatches| mismatches.map(|list| list.into_iter().map(wit_mismatch).collect()))
    }
    fn watch(
        &mut self,
        dimension: String,
        x: i32,
        y: i32,
        z: i32,
        id: String,
    ) -> Result<Option<u32>, String> {
        let key = (dimension, [x, y, z]);
        let table = self.template_table();
        let structure = table
            .as_deref()
            .and_then(|t| t.of_mod(&self.id).iter().find(|s| s.id == id))
            .ok_or_else(|| format!("no structure {id} of this mod"))?;
        self.with_store(|s, mod_id| {
            if s.get(mod_id, &key).is_none() {
                return Err(format!(
                    "no block entity of this mod at {x} {y} {z} in {}",
                    key.0
                ));
            }
            s.multiblocks.watch(mod_id, key.clone(), &id)
        })??;
        // Where the world is lent (the server's hooks), the answer is of
        // now: checked here, kept in the store, and the next check pass
        // tells the mod it came together — or that it no longer holds.
        let Some(whole) = (|| {
            let world = self.world.lock().ok()?;
            let view = world.as_ref()?;
            Some((0..4u32).find(|&quarter| {
                matches!(
                    crate::multiblocks::check_cells(structure, key.1, quarter, &|bx, by, bz| view
                        .block(&key.0, bx, by, bz)),
                    Ok(None)
                )
            }))
        })() else {
            // Without the world lent: what the last check pass saw.
            return Ok(self.last_whole(&key, &id));
        };
        let quarter = whole.unwrap_or_else(|| self.last_whole(&key, &id).unwrap_or(0));
        self.with_store(|s, m| {
            s.multiblocks.set(
                m,
                &key,
                &id,
                crate::multiblocks::Watch {
                    quarter,
                    formed: whole.is_some(),
                    told: false,
                },
            )
        })?;
        Ok(whole)
    }
}
impl State {
    /// The multiblocks table this run loaded, if the game gave it one.
    fn template_table(&self) -> Option<Arc<crate::multiblocks::Templates>> {
        self.multiblocks.lock().ok().and_then(|s| s.clone())
    }
    /// What the last check pass saw of a watch: its quarter turn where the
    /// structure held, none elsewhere (or with no watch at all).
    fn last_whole(&mut self, key: &crate::block_entities::Key, id: &str) -> Option<u32> {
        self.with_store(|s, m| s.multiblocks.get(m, key, id))
            .ok()
            .flatten()
            .filter(|w| w.formed)
            .map(|w| w.quarter)
    }
}

/// A stored structure as the component reads it.
fn wit_structure(s: crate::multiblocks::Structure) -> Structure {
    Structure {
        id: s.id,
        width: s.width,
        height: s.height,
        depth: s.depth,
        anchor: (s.anchor[0], s.anchor[1], s.anchor[2]),
        cells: s
            .cells
            .into_iter()
            .map(|c| Cell {
                dx: c.dx,
                dy: c.dy,
                dz: c.dz,
                blocks: c.blocks,
            })
            .collect(),
        data: s.data,
    }
}

/// A stored mismatch as the component reads it.
fn wit_mismatch(m: crate::multiblocks::Mismatch) -> Mismatch {
    Mismatch {
        x: m.x,
        y: m.y,
        z: m.z,
        expected: m.expected,
        found: m.found,
    }
}

/// The store of the mod's own data on the world (capability `saved-data`):
/// the block entities' store holds it, lent in the same server's hooks, so
/// a call outside them is an error, not a trap.
impl saved_data::Host for State {
    fn get(&mut self, key: String) -> Option<Vec<u8>> {
        self.with_store(|s, id| s.saved_data.get(id, &key).map(<[u8]>::to_vec))
            .ok()
            .flatten()
    }
    fn set(&mut self, key: String, data: Vec<u8>) -> Result<(), String> {
        if !storage_key_ok(&key) {
            return Err(format!("{key:?} is not [a-z0-9_.-]{{1,64}}"));
        }
        self.with_store(|s, id| s.saved_data.set(id, &key, data))?
    }
    fn delete(&mut self, key: String) {
        let _ = self.with_store(|s, id| s.saved_data.delete(id, &key));
    }
    fn keys(&mut self) -> Vec<String> {
        self.with_store(|s, id| {
            s.saved_data
                .keys(id)
                .into_iter()
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
    }
}

/// Capability `item-hooks`' reads and writes of the mod's own bytes on the
/// players' stacks, through the inventories lent for the call: a call
/// outside the server's hooks is an error, not a trap.
impl item_hooks::Host for State {
    fn stack(&mut self, player: String, slot: u32) -> Option<HeldStack> {
        let view = self.player_items.lock().ok()?;
        let view = view.as_ref()?;
        let (item, count, data) = view.stack(&player, slot, &self.id)?;
        self.item_claims
            .contains(&item)
            .then_some(HeldStack { item, count, data })
    }
    fn set_data(&mut self, player: String, slot: u32, data: Option<Vec<u8>>) -> Result<(), String> {
        if self.item_writes >= SET_DATA_PER_CALL {
            return Err(format!("at most {SET_DATA_PER_CALL} set-data calls a call"));
        }
        let data = data.unwrap_or_default();
        if data.len() > STACK_DATA {
            return Err(format!(
                "{} bytes: at most {STACK_DATA} on one stack",
                data.len()
            ));
        }
        let mut view = self
            .player_items
            .lock()
            .map_err(|_| "the players' stacks are unavailable".to_string())?;
        let view = view
            .as_mut()
            .ok_or("the players' stacks are kept only during server hooks")?;
        let (item, count, _) = view
            .stack(&player, slot, &self.id)
            .ok_or(format!("slot {slot} of {player} holds no item"))?;
        if !self.item_claims.contains(&item) {
            return Err(format!("slot {slot} of {player} holds no item of this mod"));
        }
        // A split copies a stack's components to both halves, so charge on a
        // splittable stack would double: a stack that carries data keeps
        // count 1.
        if count > 1 {
            return Err(format!(
                "a stack of {count} {item}: data is kept on one item, not a stack"
            ));
        }
        view.set_data(&player, slot, &self.id, &data)?;
        self.item_writes += 1;
        Ok(())
    }
}

impl players::Host for State {
    fn online(&mut self) -> Vec<ServerPlayer> {
        self.snapshot.players.clone()
    }
    fn tell(&mut self, player: String, text: String) -> bool {
        self.snapshot.players.iter().any(|p| p.id == player)
            && self.say(|name| Output::Tell {
                name,
                player,
                text: clip(text),
            })
    }
    fn broadcast(&mut self, text: String) {
        self.say(|name| Output::Broadcast {
            name,
            text: clip(text),
        });
    }
}
/// A key a mod registered (capability `input`).
#[derive(Clone, Debug, PartialEq)]
pub struct ModKey {
    /// The mod's own name for it, echoed in `key-down`.
    pub name: String,
    /// What it does, for Controls.
    pub label: String,
    /// `key.keyboard.r`.
    pub key: String,
}

/// What capability `input` holds for one mod.
#[derive(Default)]
struct InputState {
    claimed: Vec<String>,
    keys: Vec<ModKey>,
    events: std::collections::VecDeque<InputEvent>,
    /// The mod's keys held now, by name.
    down: Vec<String>,
    /// It holds the mouse wheel.
    wheel: bool,
    /// The keys the player chose, `name → key`: they win over
    /// `register-key`'s.
    bindings: Vec<(String, String)>,
}
/// Events kept for `poll`, items claimed and keys, per mod.
const INPUT_EVENTS: usize = 256;
const CLAIMS: usize = 64;
const KEYS: usize = 16;

/// `key.keyboard.<name>` or `key.mouse.<name>`, as options.txt writes keys;
/// the game knows which names exist and ignores a key it has none for.
fn key_ok(key: &str) -> bool {
    let rest = key
        .strip_prefix("key.keyboard.")
        .or_else(|| key.strip_prefix("key.mouse."));
    rest.is_some_and(|r| {
        (1..=32).contains(&r.len())
            && r.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.')
    })
}

impl InputState {
    /// Key `name` is `key` now; held, it lets go first.
    fn rebind(&mut self, name: &str, key: String) {
        let Some(k) = self.keys.iter_mut().find(|k| k.name == name) else {
            return;
        };
        k.key = key;
        if let Some(i) = self.down.iter().position(|n| n == name) {
            let name = self.down.swap_remove(i);
            self.push(InputEvent::KeyUp(name));
        }
    }
    fn push(&mut self, event: InputEvent) {
        if self.events.len() == INPUT_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}

impl input::Host for State {
    fn claim_items(&mut self, ids: Vec<String>) {
        self.input.claimed = ids.into_iter().take(CLAIMS).map(clip).collect();
    }
    fn register_key(&mut self, name: String, label: String, key: String) -> Result<(), String> {
        if !key_ok(&key) {
            return Err(format!(
                "{key:?} is not a key as options.txt writes them (key.keyboard.r)"
            ));
        }
        let (name, label) = (clip(name), clip(label));
        let key = match self.input.bindings.iter().find(|(n, _)| *n == name) {
            Some((_, chosen)) => chosen.clone(),
            None => key,
        };
        if let Some(k) = self.input.keys.iter_mut().find(|k| k.name == name) {
            (k.label, k.key) = (label, key);
            return Ok(());
        }
        if self.input.keys.len() == KEYS {
            return Err(format!("a mod may register {KEYS} keys"));
        }
        self.input.keys.push(ModKey { name, label, key });
        Ok(())
    }
    fn held(&mut self) -> Option<HeldItem> {
        self.snapshot.held.clone()
    }
    fn poll(&mut self) -> Vec<InputEvent> {
        self.input.events.drain(..).collect()
    }
    fn mapping(&mut self, name: String) -> Option<input::MappingState> {
        let (_, label, down) = self.snapshot.mappings.iter().find(|m| m.0 == name)?;
        Some(input::MappingState {
            label: label.clone(),
            down: *down,
        })
    }
    fn clicks_per_second(&mut self) -> (u32, u32) {
        self.snapshot.cps
    }
    fn claim_wheel(&mut self, on: bool) {
        self.input.wheel = on;
    }
}

/// HUD shapes one mod may keep (capability `render`).
const SHAPES: usize = 1024;
/// The longest jolt `render.shake` makes, in seconds.
const SHAKE_SECONDS: f32 = 10.0;
/// `render.set-zoom`'s limits: `Camera.tickFov` keeps the modifier at 0.1
/// or more, and turning never stops.
const ZOOM_MAX: f32 = 10.0;
const TURN_MIN: f32 = 0.05;
/// `set-minimap`'s side in GUI pixels and its blocks per map pixel.
const MINIMAP_SIZE: (f32, f32) = (16.0, 256.0);
const MINIMAP_SCALE: (f32, f32) = (0.5, 4.0);
/// `set-far-terrain`'s distance in chunks, as the LOD Distance setting's.
const FAR_TERRAIN_DISTANCE: (u32, u32) = (64, 1024);

/// A mod's corner panel: shapes in a `width` × `height` box the game
/// places in `corner`, stacked with the other overlays.
#[derive(Clone, Debug)]
pub struct CornerPanel {
    pub corner: RenderCorner,
    pub width: f32,
    pub height: f32,
    pub shapes: Vec<Shape>,
}

fn clip_shapes(shapes: Vec<Shape>) -> Vec<Shape> {
    shapes
        .into_iter()
        .take(SHAPES)
        .map(|shape| match shape {
            Shape::Sprite(mut s) => {
                s.sprite = clip(s.sprite);
                Shape::Sprite(s)
            }
            Shape::Item(mut i) => {
                i.item = clip(i.item);
                i.scale = if i.scale.is_finite() {
                    i.scale.clamp(0.0, 16.0)
                } else {
                    1.0
                };
                Shape::Item(i)
            }
            Shape::Text(mut t) => {
                t.text = clip(t.text);
                t.color &= 0xFF_FFFF;
                Shape::Text(t)
            }
            other => other,
        })
        .collect()
}

impl render::Host for State {
    fn set_hud(&mut self, shapes: Vec<Shape>) {
        self.shapes = clip_shapes(shapes);
    }
    fn set_corner(&mut self, corner: RenderCorner, width: f32, height: f32, shapes: Vec<Shape>) {
        let size = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1024.0)
            } else {
                0.0
            }
        };
        self.corner = Some(CornerPanel {
            corner,
            width: size(width),
            height: size(height),
            shapes: clip_shapes(shapes),
        });
    }
    fn clear_corner(&mut self) {
        self.corner = None;
    }
    fn text_width(&mut self, text: String) -> f32 {
        let widths = &self.snapshot.font_widths;
        text.chars()
            .map(|c| widths.get(c as usize).copied().unwrap_or(6.0))
            .sum::<f32>()
            .ceil()
    }
    fn set_zoom(&mut self, zoom: ModZoom) {
        let finite = |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { lo };
        let factor = finite(zoom.factor, 1.0, ZOOM_MAX);
        self.zoom = (factor > 1.0).then(|| ModZoom {
            factor,
            smooth: zoom.smooth,
            turn: if zoom.turn.is_finite() {
                zoom.turn.clamp(TURN_MIN, 1.0)
            } else {
                1.0
            },
        });
    }
    fn set_minimap(&mut self, minimap: Option<ModMinimap>) {
        let finite = |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { lo };
        self.minimap = minimap.map(|m| ModMinimap {
            size: finite(m.size, MINIMAP_SIZE.0, MINIMAP_SIZE.1),
            blocks_per_pixel: finite(m.blocks_per_pixel, MINIMAP_SCALE.0, MINIMAP_SCALE.1),
            ..m
        });
    }
    fn set_far_terrain(&mut self, far_terrain: Option<ModFarTerrain>) {
        self.far_terrain = far_terrain.map(|f| ModFarTerrain {
            distance: f
                .distance
                .clamp(FAR_TERRAIN_DISTANCE.0, FAR_TERRAIN_DISTANCE.1),
            ..f
        });
    }
    fn frame_stats(&mut self) -> FrameRate {
        let (fps, min, low) = self.snapshot.frame;
        FrameRate { fps, min, low }
    }
    fn gui_size(&mut self) -> (f32, f32) {
        self.snapshot.gui_size
    }
    fn shake(&mut self, strength: f32, seconds: f32) {
        let ok = |v: f32, max: f32| {
            if v.is_finite() {
                v.clamp(0.0, max)
            } else {
                0.0
            }
        };
        let (strength, seconds) = (ok(strength, 1.0), ok(seconds, SHAKE_SECONDS));
        if strength > 0.0 && seconds > 0.0 {
            self.out.push(Output::Shake { strength, seconds });
        }
    }
}

impl command::Host for State {
    fn run(&mut self, line: String) {
        let queued = self
            .out
            .iter()
            .filter(|o| matches!(o, Output::Command { .. }))
            .count();
        if queued < CHAT_PER_CALL {
            self.out.push(Output::Command { line: clip(line) });
        }
    }
    fn run_quiet(&mut self, lines: Vec<String>) {
        let queued: usize = self
            .out
            .iter()
            .map(|o| match o {
                Output::QuietCommands { lines, .. } => lines.len(),
                _ => 0,
            })
            .sum();
        let room = QUIET_PER_CALL.saturating_sub(queued);
        if room > 0 && !lines.is_empty() {
            self.out.push(Output::QuietCommands {
                mod_id: self.id.clone(),
                lines: lines.into_iter().take(room).map(clip).collect(),
            });
        }
    }
}
impl hud::Host for State {
    fn set_panel(&mut self, corner: Corner, lines: Vec<Line>) {
        let lines = lines
            .into_iter()
            .take(HUD_LINES)
            .map(|l| Line {
                text: clip(l.text),
                color: l.color & 0xFF_FFFF,
            })
            .collect();
        self.panel = Some(Panel { corner, lines });
    }
    fn clear(&mut self) {
        self.panel = None;
    }
}
impl ui::Host for State {
    fn open_screen(&mut self, mut screen: ui::Screen) {
        screen.title = clip(screen.title);
        screen.widgets = clip_widgets(screen.widgets);
        self.ui.screen = Some(screen);
        self.ui.open = true;
    }
    fn close_screen(&mut self) {
        self.ui.screen = None;
        self.ui.close = true;
    }
    fn set_settings(&mut self, widgets: Vec<ui::Widget>) {
        self.ui.settings = clip_widgets(widgets);
    }
}
impl storage::Host for State {
    fn get(&mut self, key: String) -> Option<Vec<u8>> {
        storage_key_ok(&key)
            .then(|| std::fs::read(self.data_dir.join(&key)).ok())
            .flatten()
    }
    fn set(&mut self, key: String, value: Vec<u8>) -> Result<(), String> {
        if !storage_key_ok(&key) {
            return Err(format!("{key:?} is not [a-z0-9_.-]{{1,64}}"));
        }
        let len = value.len() as u64;
        if len > self.storage_budget {
            return Err("too much storage traffic in one call".into());
        }
        // Everything already on disk but the file this call replaces.
        let existing: Vec<std::fs::DirEntry> = std::fs::read_dir(&self.data_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy() != key)
            .collect();
        if existing.len() >= STORAGE_MAX_KEYS {
            return Err(format!("would hold over {STORAGE_MAX_KEYS} keys"));
        }
        let used: u64 = existing
            .iter()
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum();
        if used + len > STORAGE_QUOTA {
            return Err(format!(
                "would use over {} MiB of storage",
                STORAGE_QUOTA >> 20
            ));
        }
        std::fs::create_dir_all(&self.data_dir).map_err(|e| e.to_string())?;
        std::fs::write(self.data_dir.join(&key), value).map_err(|e| e.to_string())?;
        self.storage_budget -= len;
        Ok(())
    }
    fn delete(&mut self, key: String) {
        if storage_key_ok(&key) {
            let _ = std::fs::remove_file(self.data_dir.join(&key));
        }
    }
}

/// Gives a new block entity at `pos` to each mod in `claims` that claims
/// `state`'s block.
pub fn claim(
    store: &mut crate::BlockEntities,
    claims: &[(String, Vec<String>)],
    dimension: &str,
    pos: [i32; 3],
    state: &str,
) {
    let name = crate::block_entities::block_name(state);
    for (id, blocks) in claims {
        if blocks.iter().any(|b| b == name) {
            store.create(id, (dimension.to_owned(), pos));
        }
    }
}

/// A storage key: one path segment of `[a-z0-9_.-]{1,64}`, not `.` or `..`.
/// No `/`, so a key can never leave the mod's own directory.
fn storage_key_ok(key: &str) -> bool {
    let chars_ok = (1..=64).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b));
    chars_ok && key != "." && key != ".."
}
