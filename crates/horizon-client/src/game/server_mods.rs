//! Server mods in the game (`docs/mods.md`, "Server mods"): the game that
//! simulates the world (in-frame single player, the server thread, a LAN
//! host, the dedicated server) runs them. At the end of each tick it tells
//! them who joined and left, then what players said, broke and placed since
//! the last one, then ticks them; a player's command reaches them at once.
//! Their lines go to the players they name. With no server mod on, each hook
//! here is one check.
use super::{Game, lan::LanHost};
use horizon_mods::items::Stack;
#[cfg(not(feature = "mods"))]
use horizon_world::registry::BlockStateId;

/// Who did something a server mod hears about, resolved at the tick's end.
#[cfg_attr(not(feature = "mods"), allow(dead_code))]
pub(super) enum Who {
    /// The window's own player, in-frame.
    Local,
    /// A connection's player (the server thread's owner is one).
    Conn(u32),
    Uuid(u128),
}

/// A server mod's menu open for a connection (capability `menus`).
#[cfg_attr(not(feature = "mods"), allow(dead_code))]
pub(super) struct ModMenu {
    pub(super) conn: u32,
    /// The player's id, for `closed` once they have left.
    pub(super) player: String,
    pub(super) window: i32,
    pub(super) mod_id: String,
    pub(super) menu: String,
    /// The mod's own items (capability `menus`), slot by slot.
    pub(super) items: Vec<Option<horizon_client::items::ItemStack>>,
    /// `open-inventory`'s bound slots, and the slots last sent.
    #[cfg(feature = "mods")]
    pub(super) inventory: Option<horizon_mods::MenuInventory>,
    pub(super) sent: Vec<Option<horizon_client::items::ItemStack>>,
}

/// Capability `entities`' bookkeeping: which mobs of the game are a mod's
/// (found by the marks in their saved data), and what the mods were last
/// told of them.
#[cfg(feature = "mods")]
#[derive(Default)]
pub(super) struct ModEntityBook {
    pub(super) owned: std::collections::HashMap<u32, OwnedEntity>,
    /// Mobs looked at that are no mod's.
    pub(super) plain: std::collections::HashSet<u32>,
    /// The world folder the book is of.
    pub(super) world: Option<std::path::PathBuf>,
    /// Told on the next tick: mobs that started dying, and mobs gone.
    pub(super) died: Vec<horizon_mods::EntityInfo>,
    pub(super) removed: Vec<horizon_mods::EntityInfo>,
}
#[cfg(feature = "mods")]
pub(super) struct OwnedEntity {
    pub(super) takes_use: bool,
    /// Its state as the last scan found it.
    pub(super) last: horizon_mods::EntityInfo,
    /// `died` has been told.
    pub(super) died: bool,
}

/// A server mod's display entity (capability `displays`): sent as packets
/// to the players in its dimension, never in the world itself.
#[cfg_attr(not(feature = "mods"), allow(dead_code))]
pub(super) struct ModDisplay {
    pub(super) id: i32,
    pub(super) mod_id: String,
    pub(super) name: String,
    pub(super) dimension: String,
    pub(super) pos: [f64; 3],
    /// `minecraft:block_display`, `minecraft:item_display` or
    /// `minecraft:text_display`.
    pub(super) kind: &'static str,
    pub(super) meta: Vec<(u8, i32, horizon_protocol::entity_data::Value)>,
    /// The connections that have it.
    pub(super) sent: Vec<u32>,
    /// The mob that carries it (`ride`): its entity id, `None` while it
    /// stands on its own.
    pub(super) riding: Option<u32>,
}

/// What players did during a tick, for the server mods at its end.
#[cfg_attr(not(feature = "mods"), allow(dead_code))]
pub(super) enum ModEvent {
    Chat(Who, String),
    /// A click in a mod's menu slot: the mod, its menu, the slot, the
    /// button and shift.
    MenuClick(Who, String, String, u32, u8, bool),
    /// A mod's menu closed by its player.
    MenuClosed(Who, String, String),
    /// A player used a block a mod takes the use of: the mod, the position
    /// and the block.
    BlockUse(Who, String, [i32; 3], String),
    /// A message of a mod's client half: the mod, the channel and the bytes.
    NetFromClient(Who, String, String, Vec<u8>),
    /// A player right-clicked a mod's mob (capability `entities`).
    EntityUse(Who, u32),
    /// A player's blow killed a mob (capability `game-events`): its kind and
    /// entity id.
    MobKilled(Who, String, u32),
    /// A player used (right-clicked) an item a mod takes the behaviour of
    /// (capability `item-hooks`): the mod, the stack as the mod's
    /// inventories keep it, and the block under the crosshair if there was
    /// one (its position and state). The mod's own bytes on the stack are
    /// read out of `components` at the tick's end, one read per mod.
    ItemUse(Who, String, Stack, Option<([i32; 3], String)>),
    Block {
        who: Who,
        pos: [i32; 3],
        state: String,
        placed: bool,
    },
}

#[cfg(not(feature = "mods"))]
impl Game<'_> {
    pub(super) fn tick_server_mods(&mut self) {}
    pub(super) fn mod_block_event(&mut self, _pos: [i32; 3], _state: BlockStateId, _placed: bool) {}
    pub(super) fn mod_player_chat(&mut self, _who: Who, _text: &str) {}
    pub(super) fn local_server_mod_command(&mut self, _line: &str) -> bool {
        false
    }
    pub(super) fn guest_server_mod_command(
        &mut self,
        _host: &mut LanHost,
        _conn: u32,
        _line: &str,
    ) -> bool {
        false
    }
    pub(super) fn apply_mod_block_writes(&mut self) {}
    pub(super) fn save_mod_block_entities(&mut self) {}
    pub(super) fn lan_mod_menu_click(
        &mut self,
        _host: &mut LanHost,
        _conn: u32,
        _p: &horizon_protocol::packets::play::ContainerClick,
    ) -> bool {
        false
    }
    pub(super) fn lan_mod_menu_closed(&mut self, _conn: u32, _window: i32) -> bool {
        false
    }
    pub(super) fn mod_block_use(&mut self, _pos: [i32; 3]) -> bool {
        false
    }
    pub(super) fn guest_mod_net(&mut self, _conn: u32, _data: &[u8]) {}
    pub(in crate::game) fn mod_entity_hurt(&mut self, _index: usize, damage: f32) -> f32 {
        damage
    }
    pub(in crate::game) fn mod_note_death(&mut self, _uuid: Option<u128>, _cause: String) {}
    pub(in crate::game) fn mod_mob_killed(&mut self, _index: usize) {}
    pub(in crate::game) fn mod_entity_use(&mut self, _index: usize) -> bool {
        false
    }
    pub(in crate::game) fn mod_item_use(&mut self, _block_hit: Option<&super::Hit>) -> bool {
        false
    }
    pub(in crate::game) fn mod_item_blow(
        &mut self,
        _id: u32,
        _kind: &str,
        _health: f32,
        damage: f32,
    ) -> f32 {
        damage
    }
    pub(in crate::game) fn mod_pvp_blow(
        &mut self,
        _host: &mut LanHost,
        _attacker: Option<u32>,
        _weapon: &horizon_client::items::ItemStack,
        _id: u32,
        _health: f32,
        damage: f32,
    ) -> f32 {
        damage
    }
    pub(in crate::game) fn mod_armor_absorb(&mut self, _source: &str, damage: f32) -> f32 {
        damage
    }
}

#[cfg(feature = "mods")]
mod run {
    use super::super::Hit;
    use super::ModDisplay;
    use super::{Game, LanHost, ModEvent, ModMenu, Who};
    use horizon_client::chunk_map::ChunkMap;
    use horizon_client::items::ItemStack;
    use horizon_client::lan::containers::ModSlot;
    use horizon_mods::{
        DisplayShape, HeldStack, ItemEntity, PlayerItems, PlayerStack, SlotMode, TICK_ITEMS,
        Transform, items::Stack,
    };
    use horizon_protocol::entity_data::{Value as Data, serializer};

    /// Messages a player may send to the mods in one tick.
    const NET_PER_TICK: u32 = 32;

    /// Mods' display entities take ids from here up, past the world's.
    const FIRST_DISPLAY_ID: i32 = 0x6000_0000;

    /// Displays one mob may carry (`ride`): past them the ride is dropped,
    /// as `show` past its displays.
    const RIDERS_PER_MOB: usize = 16;

    /// A display's data (`Display`'s, then `BlockDisplay`'s or
    /// `TextDisplay`'s): its transform, over `interpolation` ticks.
    fn display_meta(
        block: Option<i32>,
        text: Option<horizon_world::nbt::Tag>,
        t: &Transform,
        interpolation: u32,
    ) -> Vec<(u8, i32, Data)> {
        let (tx, ty, tz) = t.translation;
        let (sx, sy, sz) = t.scale;
        let (rx, ry, rz, rw) = t.rotation;
        let mut meta = vec![
            (8, serializer::INT, Data::Int(0)),
            (9, serializer::INT, Data::Int(interpolation as i32)),
            (11, serializer::VECTOR3, Data::Floats(vec![tx, ty, tz])),
            (12, serializer::VECTOR3, Data::Floats(vec![sx, sy, sz])),
            (
                13,
                serializer::QUATERNION,
                Data::Floats(vec![rx, ry, rz, rw]),
            ),
        ];
        if let Some(state) = block {
            meta.push((23, serializer::BLOCK_STATE, Data::Id(state)));
        }
        if let Some(text) = text {
            // Billboard CENTER: the text faces whoever looks at it.
            meta.push((15, serializer::BYTE, Data::Byte(3)));
            meta.push((23, serializer::COMPONENT, Data::Text(text)));
        }
        meta
    }
    use horizon_client::{
        command::args::GameMode as Mode,
        lan::data::component_nbt,
        text::{self, Text},
    };
    use horizon_mods::{
        ArgumentKind, BlockPos, GameMode, MenuItem, Output, ServerCommand, ServerPlayer, WorldView,
    };
    use horizon_protocol::packets::play::{self, Clientbound as C};
    use horizon_world::registry::{BlockRegistry, BlockStateId};
    use serde_json::{Value, json};

    /// Blocks queued for levels that don't tick (another dimension, in-frame)
    /// are kept up to this many; the oldest go first.
    const MAX_QUEUED_WRITES: usize = 1 << 16;

    /// Capability `world`'s view during a call: the levels' chunks, moved out
    /// of the game for the call and back after it (no copy).
    struct LentWorld {
        registry: &'static BlockRegistry,
        biomes: std::sync::Arc<horizon_client::biome::Biomes>,
        dims: Vec<(&'static str, ChunkMap)>,
        max_stacks: std::sync::Arc<std::collections::HashMap<String, u32>>,
    }
    impl WorldView for LentWorld {
        fn max_stack(&self, item: &str) -> Option<u32> {
            self.max_stacks.get(item).copied()
        }
        fn block(&self, dimension: &str, x: i32, y: i32, z: i32) -> Option<String> {
            let (_, chunks) = self.dims.iter().find(|(d, _)| *d == dimension)?;
            let chunk = chunks.get(&(x >> 4, z >> 4))?;
            Some(state_string(self.registry, chunk.block(x & 15, y, z & 15)))
        }
        fn biome(&self, dimension: &str, x: i32, y: i32, z: i32) -> Option<String> {
            let (_, chunks) = self.dims.iter().find(|(d, _)| *d == dimension)?;
            let chunk = chunks.get(&(x >> 4, z >> 4))?;
            Some(self.biomes.get(chunk.biome(x, y, z)).name.clone())
        }
        fn has_dimension(&self, dimension: &str) -> bool {
            matches!(
                dimension,
                "minecraft:overworld" | "minecraft:the_nether" | "minecraft:the_end"
            )
        }
        fn check_state(&self, state: &str) -> Result<(), String> {
            crate::game::render_control::parse_state(self.registry, state)
                .map(drop)
                .map_err(|e| format!("{state}: {e}"))
        }
    }

    /// Server mods' commands as a root of the commands report
    /// (`lan::commands`): a literal per command, then its arguments in a
    /// chain, each node runnable where only optional arguments follow.
    fn command_tree<'a>(commands: impl Iterator<Item = &'a ServerCommand>) -> Value {
        let mut root = serde_json::Map::new();
        for c in commands {
            let mut next: Option<(String, Value)> = None;
            for (i, a) in c.arguments.iter().enumerate().rev() {
                let executable = c.arguments.get(i + 1).is_none_or(|b| b.optional);
                let (parser, properties) = match a.kind {
                    ArgumentKind::Word => ("brigadier:string", json!({"type": "word"})),
                    ArgumentKind::Text => ("brigadier:string", json!({"type": "greedy"})),
                    ArgumentKind::Integer((min, max)) => {
                        let mut p = serde_json::Map::new();
                        if let Some(min) = min {
                            p.insert("min".into(), min.into());
                        }
                        if let Some(max) = max {
                            p.insert("max".into(), max.into());
                        }
                        ("brigadier:integer", Value::Object(p))
                    }
                    ArgumentKind::Number => ("brigadier:double", Value::Null),
                    ArgumentKind::Boolean => ("brigadier:bool", Value::Null),
                    ArgumentKind::Player => (
                        "minecraft:entity",
                        json!({"amount": "single", "type": "players"}),
                    ),
                    ArgumentKind::BlockPos => ("minecraft:block_pos", Value::Null),
                };
                let mut node = json!({
                    "type": "argument",
                    "parser": parser,
                    "properties": properties,
                    "executable": executable,
                });
                if let Some((name, child)) = next.take() {
                    node["children"] = json!({ name: child });
                }
                next = Some((a.name.clone(), node));
            }
            let executable = c.arguments.first().is_none_or(|a| a.optional);
            let mut literal = json!({"type": "literal", "executable": executable});
            if let Some((name, child)) = next {
                literal["children"] = json!({ name: child });
            }
            root.insert(c.name.clone(), literal);
        }
        if root.is_empty() {
            return Value::Null;
        }
        json!({"type": "root", "children": root})
    }

    /// `minecraft:oak_log[axis=y]`.
    fn state_string(registry: &BlockRegistry, state: BlockStateId) -> String {
        let Some(s) = registry.state(state) else {
            return "minecraft:air".into();
        };
        if s.properties.is_empty() {
            return s.name.clone();
        }
        let props: Vec<String> = s
            .properties
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        format!("{}[{}]", s.name, props.join(","))
    }

    /// `UUID.toString()`.
    fn uuid_string(u: u128) -> String {
        let h = format!("{u:032x}");
        format!(
            "{}-{}-{}-{}-{}",
            &h[..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..]
        )
    }

    /// The key of a mod's own bytes on a stack (capability `item-hooks`): a
    /// component on the stack, so the bytes travel with it between slots,
    /// players and the world's saves.
    fn mod_data_key(mod_id: &str) -> String {
        format!("horizon:{mod_id}")
    }

    /// The mod's own bytes on a stack: empty for none.
    fn mod_stack_data(stack: &ItemStack, mod_id: &str) -> Vec<u8> {
        stack
            .data()
            .and_then(|d| d.other(&mod_data_key(mod_id)))
            .and_then(|tag| match tag {
                horizon_world::nbt::Tag::ByteArray(b) => Some(b.as_slice()),
                _ => None,
            })
            .map_or_else(Vec::new, ToOwned::to_owned)
    }

    /// Replaces the mod's own bytes on a stack (empty removes them).
    fn set_mod_stack_data(stack: &mut ItemStack, mod_id: &str, data: &[u8]) {
        let value = (!data.is_empty()).then(|| horizon_world::nbt::Tag::ByteArray(data.to_vec()));
        stack.data_mut().set_other(&mod_data_key(mod_id), value);
        stack.tidy();
    }

    /// The mod's own bytes in a stack's components string, as `store_stack`
    /// wrote them: the bytes the stack carried when the event was taken.
    fn mod_data_of(components: &str, mod_id: &str) -> Vec<u8> {
        horizon_client::item_data::command_components(components)
            .ok()
            .and_then(|tag| tag.get(mod_data_key(mod_id).as_str()).cloned())
            .and_then(|tag| match tag {
                horizon_world::nbt::Tag::ByteArray(b) => Some(b),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// A player's stacks as capability `item-hooks` numbers slots: the 36
    /// inventory slots (hotbar first), then armour feet to head 36..39,
    /// then the off hand 40. The game keeps armour head..feet.
    fn slots_of(
        inventory: &[Option<ItemStack>],
        armor: &[Option<ItemStack>; 4],
        offhand: &Option<ItemStack>,
    ) -> Vec<Option<ItemStack>> {
        let mut slots = inventory.to_vec();
        slots.extend(armor.iter().rev().cloned());
        slots.push(offhand.clone());
        slots
    }

    /// Writes into one of `slots_of`'s slots of the stacks the game keeps.
    fn write_player_slot(
        inventory: &mut [Option<ItemStack>],
        armor: &mut [Option<ItemStack>; 4],
        offhand: &mut Option<ItemStack>,
        slot: u32,
        stack: Option<ItemStack>,
    ) {
        match slot {
            s if (36..40).contains(&s) => armor[3 - (s - 36) as usize] = stack,
            40 => *offhand = stack,
            s => {
                if let Some(dst) = inventory.get_mut(s as usize) {
                    *dst = stack;
                }
            }
        }
    }

    /// The players' stacks for the mods during a call (capability
    /// `item-hooks`): copies lent in, the `set-data` writes applied when
    /// the game takes them back.
    struct LentItems {
        /// Each player on the server, by id, in `slots_of`'s numbering.
        players: Vec<(String, Vec<Option<ItemStack>>)>,
        /// The slots `set-data` wrote, by player and slot.
        written: Vec<(String, u32)>,
    }

    impl PlayerItems for LentItems {
        fn stack(&self, player: &str, slot: u32, mod_id: &str) -> Option<(String, u32, Vec<u8>)> {
            let slots = self.players.iter().find(|(p, _)| p == player)?.1.as_slice();
            let stack = slots.get(slot as usize)?.as_ref()?;
            Some((
                stack.item.to_string(),
                stack.count,
                mod_stack_data(stack, mod_id),
            ))
        }
        fn set_data(
            &mut self,
            player: &str,
            slot: u32,
            mod_id: &str,
            data: &[u8],
        ) -> Result<(), String> {
            let slots = self
                .players
                .iter_mut()
                .find(|(p, _)| p == player)
                .map(|(_, s)| s.as_mut_slice())
                .ok_or_else(|| format!("no player {player} on the server"))?;
            let stack = slots
                .get_mut(slot as usize)
                .and_then(Option::as_mut)
                .ok_or_else(|| format!("slot {slot} of {player} holds no item"))?;
            set_mod_stack_data(stack, mod_id, data);
            self.written.push((player.to_owned(), slot));
            Ok(())
        }
    }

    /// Every player on the server's stacks, by id: the window's own player
    /// in-frame, then the hub's guests — the guest whose turn it is holds
    /// the host's stacks in their record and their own in the game, as
    /// `enter_guest` swapped them. `host` is the hub when it is out of
    /// `self.lan`.
    fn player_stacks(game: &Game, host: Option<&LanHost>) -> Vec<(String, Vec<Option<ItemStack>>)> {
        let local = game.options.execution_role.is_client()
            && game.remote.is_none()
            && game.player.is_some();
        let hub = host.or(game.lan.as_ref());
        let mut out = Vec::new();
        // The guest whose turn it is: their stacks are in the game.
        let acting = game
            .lan_acting
            .filter(|_| local)
            .and_then(|conn| hub?.hub.guest(conn))
            .map(|g| uuid_string(g.uuid));
        if let Some(id) = &acting {
            out.push((
                id.clone(),
                slots_of(&game.inventory, &game.armor, &game.offhand),
            ));
        }
        fn placed_guests(
            hub: &LanHost,
        ) -> impl Iterator<Item = &horizon_client::lan::guest::Guest> {
            hub.hub.guests.iter().filter(|g| g.placed)
        }
        if local {
            let id = game.player_uuid_string();
            if acting.as_ref() != Some(&id) {
                if let Some(conn) = game.lan_acting {
                    // While a guest acts, the host's stacks are in their
                    // record.
                    if let Some(g) = hub.and_then(|h| h.hub.guests.iter().find(|g| g.conn == conn))
                    {
                        out.push((id, slots_of(&g.inventory, &g.armor, &g.offhand)));
                    }
                } else {
                    out.push((id, slots_of(&game.inventory, &game.armor, &game.offhand)));
                }
            }
        }
        if let Some(hub) = hub {
            for g in placed_guests(hub) {
                let id = uuid_string(g.uuid);
                if out.iter().any(|(p, _)| *p == id) {
                    continue;
                }
                out.push((id, slots_of(&g.inventory, &g.armor, &g.offhand)));
            }
        }
        out
    }

    fn mode(id: i32) -> GameMode {
        match id {
            1 => GameMode::Creative,
            2 => GameMode::Adventure,
            3 => GameMode::Spectator,
            _ => GameMode::Survival,
        }
    }

    impl Game<'_> {
        /// The players on this game's server: the window's own player
        /// in-frame, then every connection's. `host` is the hub when it is
        /// out of `self.lan`.
        pub(in crate::game) fn server_players(&self, host: Option<&LanHost>) -> Vec<ServerPlayer> {
            let mut out = Vec::new();
            if self.options.execution_role.is_client()
                && self.remote.is_none()
                && let Some(p) = &self.player
            {
                out.push(ServerPlayer {
                    id: self.player_uuid_string(),
                    name: crate::game::chat::PLAYER_NAME.to_owned(),
                    x: p.position.x,
                    y: p.position.y,
                    z: p.position.z,
                    dimension: self.dim.id().to_owned(),
                    game_mode: match self.mode {
                        Mode::Survival => GameMode::Survival,
                        Mode::Creative => GameMode::Creative,
                        Mode::Adventure => GameMode::Adventure,
                        Mode::Spectator => GameMode::Spectator,
                    },
                    health: p.health.health(),
                });
            }
            let guests = host
                .or(self.lan.as_ref())
                .into_iter()
                .flat_map(|h| &h.hub.guests);
            out.extend(guests.filter(|g| g.placed).map(|g| ServerPlayer {
                id: uuid_string(g.uuid),
                name: g.name.clone(),
                x: g.position[0],
                y: g.position[1],
                z: g.position[2],
                dimension: g.dimension.clone(),
                game_mode: mode(g.game_mode),
                health: g.health,
            }));
            out
        }

        /// Whether what players do now reaches server mods: one is running
        /// and this game simulates the world.
        fn hears_players(&self) -> bool {
            self.mods.server && self.remote.is_none()
        }

        /// The player acting, as the server mods see them: the guest whose
        /// turn it is, else the window's own player.
        fn acting_server_player(&self, host: Option<&LanHost>) -> Option<ServerPlayer> {
            let players = self.server_players(host);
            let id = match self.lan_acting_uuid {
                Some(guest) => uuid_string(guest),
                None => self.player_uuid_string(),
            };
            let found = players.into_iter().find(|p| p.id == id);
            // While a guest's packet is handled the hub is out of `self.lan`
            // and the game stands in for the guest: their state is its own.
            match (found, self.lan_acting_uuid, &self.player) {
                (None, Some(guest), Some(p)) => Some(ServerPlayer {
                    id: uuid_string(guest),
                    name: self.lan_acting_name.clone().unwrap_or_default(),
                    x: p.position.x,
                    y: p.position.y,
                    z: p.position.z,
                    dimension: self.dim.id().to_owned(),
                    game_mode: match self.mode {
                        Mode::Survival => GameMode::Survival,
                        Mode::Creative => GameMode::Creative,
                        Mode::Adventure => GameMode::Adventure,
                        Mode::Spectator => GameMode::Spectator,
                    },
                    health: p.health.health(),
                }),
                (found, _, _) => found,
            }
        }

        /// Lends the players' stacks to capability `item-hooks` for the
        /// calls until [`Game::return_lent_player_items`]: copies, the
        /// writes applied on the way back. `host` is the hub when it is out
        /// of `self.lan`.
        fn lend_player_items(&mut self, host: Option<&LanHost>) {
            if !self.mods.player_items {
                return;
            }
            let players = player_stacks(self, host);
            self.mods.mods.lend_player_items(Box::new(LentItems {
                players,
                written: Vec::new(),
            }));
        }

        /// The stacks [`Game::lend_player_items`] lent, back to the game:
        /// each `set-data` write into the player it belongs to, whose
        /// client is told on their next flush. `host` is the hub when it is
        /// out of `self.lan`.
        fn return_lent_player_items(&mut self, mut host: Option<&mut LanHost>) {
            let Some(view) = self.mods.mods.take_player_items() else {
                return;
            };
            let view: Box<dyn std::any::Any> = view;
            let Ok(mut lent) = view.downcast::<LentItems>() else {
                return;
            };
            if lent.written.is_empty() {
                return;
            }
            for (id, slot) in std::mem::take(&mut lent.written) {
                let Some(stack) = lent
                    .players
                    .iter()
                    .find(|(p, _)| *p == id)
                    .and_then(|(_, slots)| slots.get(slot as usize))
                    .cloned()
                    .flatten()
                else {
                    continue;
                };
                // The lend window is the mod's own call: the game's stacks
                // stand as they were copied.
                let acting = self.lan_acting_uuid.is_some_and(|u| uuid_string(u) == id);
                if acting || (self.lan_acting.is_none() && id == self.player_uuid_string()) {
                    // The guest whose turn it is keeps their stacks in the
                    // game, as the window's own player does the rest of
                    // the time.
                    write_player_slot(
                        &mut self.inventory,
                        &mut self.armor,
                        &mut self.offhand,
                        slot,
                        Some(stack),
                    );
                } else {
                    let hub = match host.as_mut() {
                        Some(h) => Some(&mut **h),
                        None => self.lan.as_mut(),
                    };
                    if let Some(g) = hub
                        .and_then(|h| h.hub.guests.iter_mut().find(|g| uuid_string(g.uuid) == id))
                    {
                        write_player_slot(
                            &mut g.inventory,
                            &mut g.armor,
                            &mut g.offhand,
                            slot,
                            Some(stack),
                        );
                    }
                }
            }
        }

        /// Lends the mods the server's stores and the players' stacks for
        /// one synchronous call (an item's blow, armour's take), and takes
        /// them back. The world is not lent: these hooks read no blocks,
        /// and lending would move the levels' chunks for every blow.
        /// `host` is the hub when it is out of `self.lan`.
        fn with_mod_stores<R>(
            &mut self,
            host: Option<&mut LanHost>,
            f: impl FnOnce(&mut Self) -> R,
        ) -> R {
            self.read_mod_block_entities();
            let store = std::mem::take(&mut self.mods.block_entities);
            self.mods.mods.lend_block_entities(store);
            self.lend_player_items(host.as_deref());
            // A blow's mods may read and steer mobs, not spawn (the game
            // applies a spawn after the call, and this one is mid-tick).
            self.lend_mod_entities(false);
            let r = f(self);
            self.return_mod_entities();
            if let Some(store) = self.mods.mods.take_block_entities() {
                self.mods.block_entities = store;
            }
            self.return_lent_player_items(host);
            r
        }

        /// The acting player used (right-clicked) the main hand's item:
        /// true when a server mod takes the behaviour of it (capability
        /// `item-hooks`), and the game does nothing else with the click
        /// (the mod hears it at the tick's end). A sneaking player keeps
        /// vanilla's use, and a block a mod takes the use of wins over the
        /// item (`mod_block_use` already ran).
        pub(in crate::game) fn mod_item_use(&mut self, block_hit: Option<&Hit>) -> bool {
            if !self.hears_players() {
                return false;
            }
            let claims = self.mods.mods.item_use_claims();
            if claims.is_empty() {
                return false;
            }
            // A sneaking player keeps vanilla's use, as with a mod's block.
            if self.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) {
                return false;
            }
            let Some(stack) = self.stack_in(self.selected).cloned() else {
                return false;
            };
            let takers: Vec<String> = claims
                .into_iter()
                .filter(|(_, items)| items.iter().any(|i| i == stack.item.as_ref()))
                .map(|(id, _)| id)
                .collect();
            if takers.is_empty() {
                return false;
            }
            let on_block = block_hit.map(|hit| {
                let state = self.state_string(self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]));
                (hit.pos, state)
            });
            let event_stack = Self::store_stack(&stack);
            let who = || self.lan_acting.map_or(Who::Local, Who::Conn);
            for id in &takers {
                self.mods.events.push(ModEvent::ItemUse(
                    who(),
                    id.clone(),
                    event_stack.clone(),
                    on_block.clone(),
                ));
            }
            true
        }

        /// The damage of a blow with a claimed item in the main hand
        /// (capability `item-hooks`): each mod claiming the item answers,
        /// before the blow lands.
        pub(in crate::game) fn mod_item_blow(
            &mut self,
            id: u32,
            kind: &str,
            health: f32,
            damage: f32,
        ) -> f32 {
            let claims = self.mods.mods.item_use_claims();
            let Some(stack) = self.stack_in(self.selected).cloned() else {
                return damage;
            };
            let takers: Vec<String> = claims
                .into_iter()
                .filter(|(_, items)| items.iter().any(|i| i == stack.item.as_ref()))
                .map(|(id, _)| id)
                .collect();
            if takers.is_empty() || !self.hears_players() {
                return damage;
            }
            let Some(player) = self.acting_server_player(None) else {
                return damage;
            };
            // The hub is taken out for the call: its guests' stacks are
            // lent, and the mods send nothing while they answer.
            let mut host = self.lan.take();
            let damage = self.item_blow(host.as_mut(), &player, &stack, id, kind, health, damage);
            self.lan = host;
            damage
        }

        /// As `mod_item_blow`, for a player's blow on a player: the weapon
        /// and the attacker as the strike kept them (the attacker's turn
        /// may have ended by the time the blow lands).
        pub(in crate::game) fn mod_pvp_blow(
            &mut self,
            host: &mut LanHost,
            attacker: Option<u32>,
            weapon: &ItemStack,
            id: u32,
            health: f32,
            damage: f32,
        ) -> f32 {
            let claims = self.mods.mods.item_use_claims();
            if claims.is_empty() || !self.hears_players() {
                return damage;
            }
            if !claims
                .iter()
                .any(|(_, items)| items.iter().any(|i| i == weapon.item.as_ref()))
            {
                return damage;
            }
            let player = match attacker {
                Some(conn) => host.hub.guest(conn).map(|g| ServerPlayer {
                    id: uuid_string(g.uuid),
                    name: g.name.clone(),
                    x: g.position[0],
                    y: g.position[1],
                    z: g.position[2],
                    dimension: g.dimension.clone(),
                    game_mode: mode(g.game_mode),
                    health: g.health,
                }),
                None => self.acting_server_player(None),
            };
            let Some(player) = player else {
                return damage;
            };
            self.item_blow(
                Some(host),
                &player,
                weapon,
                id,
                "minecraft:player",
                health,
                damage,
            )
        }

        /// One blow's damage: the claimed item's mods answer in turn, each
        /// with the damage the last left.
        fn item_blow(
            &mut self,
            host: Option<&mut LanHost>,
            player: &ServerPlayer,
            weapon: &ItemStack,
            id: u32,
            kind: &str,
            health: f32,
            damage: f32,
        ) -> f32 {
            let takers: Vec<String> = self
                .mods
                .mods
                .item_use_claims()
                .into_iter()
                .filter(|(_, items)| items.iter().any(|i| i == weapon.item.as_ref()))
                .map(|(id, _)| id)
                .collect();
            let entity = ItemEntity {
                id,
                kind: kind.to_owned(),
                health,
            };
            let item = weapon.item.to_string();
            let count = weapon.count;
            let mut damage = damage;
            self.with_mod_stores(host, |g| {
                for mod_id in &takers {
                    let held = HeldStack {
                        item: item.clone(),
                        count,
                        data: mod_stack_data(weapon, mod_id),
                    };
                    damage = g
                        .mods
                        .mods
                        .item_hurt_entity(mod_id, player, held, &entity, damage);
                }
                damage
            })
        }

        /// The players' claimed armour takes its share of a hurt (capability
        /// `item-hooks`): the damage left after it. Called in the funnel
        /// every hurt of a named type goes through (`hurt_player_by`).
        pub(in crate::game) fn mod_armor_absorb(&mut self, source: &str, damage: f32) -> f32 {
            if !self.hears_players() || !self.mods.player_items || damage <= 0.0 {
                return damage;
            }
            let Some(player) = self.acting_server_player(None) else {
                return damage;
            };
            let mut host = self.lan.take();
            let left = self.with_mod_stores(host.as_mut(), |g| {
                damage - g.mods.mods.item_hurt(&player, damage, source)
            });
            self.lan = host;
            left
        }

        /// The tick's end: joins and leaves, the tick's events, the tick hook.
        pub(in crate::game) fn tick_server_mods(&mut self) {
            if !self.options.execution_role.is_client() {
                // A server has no frames to check its dev mods in.
                self.reload_dev_mods();
            }
            self.send_server_mod_commands();
            if !self.mods.server {
                return;
            }
            let players = self.server_players(None);
            let snapshot = self.mod_snapshot_with(players.clone());
            self.mods.mods.set_snapshot(snapshot);
            self.lend_world();
            let before = std::mem::take(&mut self.mods.players);
            for p in before
                .iter()
                .filter(|p| !players.iter().any(|q| q.id == p.id))
            {
                self.mods.mods.player_left(p);
            }
            for p in players
                .iter()
                .filter(|p| !before.iter().any(|q| q.id == p.id))
            {
                self.mods.mods.player_joined(p);
            }
            self.tell_game_events(&players, &before);
            for event in std::mem::take(&mut self.mods.events) {
                let who = |w: &Who| {
                    let id = match w {
                        Who::Local => self.player_uuid_string(),
                        Who::Uuid(u) => uuid_string(*u),
                        Who::Conn(c) => {
                            let guests = self.lan.iter().flat_map(|h| &h.hub.guests);
                            let g = guests.into_iter().find(|g| g.conn == *c);
                            g.map_or_else(String::new, |g| uuid_string(g.uuid))
                        }
                    };
                    players.iter().chain(&before).find(|p| p.id == id).cloned()
                };
                match event {
                    ModEvent::Chat(w, text) => {
                        if let Some(p) = who(&w) {
                            self.mods.mods.player_chat(&p, &text);
                        }
                    }
                    ModEvent::MenuClick(w, mod_id, menu, slot, button, shift) => {
                        if let Some(p) = who(&w) {
                            self.mods
                                .mods
                                .menu_clicked(&mod_id, &p, &menu, slot, button, shift);
                        }
                    }
                    ModEvent::MenuClosed(w, mod_id, menu) => {
                        if let Some(p) = who(&w) {
                            self.mods.mods.menu_closed(&mod_id, &p, &menu);
                        }
                    }
                    ModEvent::MobKilled(w, kind, id) => {
                        if let Some(p) = who(&w) {
                            self.mods.mods.mob_killed(&p, &kind, id);
                        }
                    }
                    ModEvent::EntityUse(w, id) => {
                        if let Some(p) = who(&w)
                            && let Some(o) = self.mods.mod_entities.owned.get(&id)
                        {
                            let info = o.last.clone();
                            self.mods.mods.entity_used(&p, &info);
                        }
                    }
                    ModEvent::NetFromClient(w, mod_id, channel, data) => {
                        if let Some(p) = who(&w) {
                            self.mods.mods.net_client_sent(&mod_id, &p, &channel, &data);
                        }
                    }
                    ModEvent::BlockUse(w, mod_id, pos, state) => {
                        if let Some(p) = who(&w) {
                            let dimension = p.dimension.clone();
                            self.mods
                                .mods
                                .block_used(&mod_id, &p, &dimension, pos, &state);
                        }
                    }
                    ModEvent::ItemUse(w, mod_id, stack, on_block) => {
                        if let Some(p) = who(&w) {
                            let held = HeldStack {
                                item: stack.item.clone(),
                                count: stack.count,
                                data: mod_data_of(&stack.components, &mod_id),
                            };
                            let block =
                                on_block.as_ref().map(|(pos, state)| (*pos, state.as_str()));
                            self.mods.mods.item_used(&mod_id, &p, held, block);
                        }
                    }
                    ModEvent::Block {
                        who: w,
                        pos,
                        state,
                        placed,
                    } => {
                        let Some(p) = who(&w) else { continue };
                        let pos = BlockPos {
                            x: pos[0],
                            y: pos[1],
                            z: pos[2],
                        };
                        if placed {
                            let at = [pos.x, pos.y, pos.z];
                            self.mods.mods.block_entity_placed(&p.dimension, at, &state);
                            self.mods.mods.block_placed(&p, pos, &state);
                        } else {
                            self.mods.mods.block_broken(&p, pos, &state);
                        }
                    }
                }
            }
            // Menus of players who left close with them.
            let (gone, open): (Vec<_>, Vec<_>) = std::mem::take(&mut self.mods.menus)
                .into_iter()
                .partition(|m| !players.iter().any(|p| p.id == m.player));
            self.mods.menus = open;
            for m in gone {
                if let Some(p) = before.iter().find(|p| p.id == m.player) {
                    self.mods.mods.menu_closed(&m.mod_id, p, &m.menu);
                }
            }
            self.mods.net_counts.clear();
            self.mods.mods.server_tick(self.ticks);
            self.mods.mods.block_entity_tick(self.ticks);
            // Capability `config`: a look at the config files every few seconds.
            self.mods.mods.config_tick(self.ticks);
            // The mods' mobs: those that started dying or are gone, then the
            // tick, all as the scan at the start of the tick found them.
            for e in std::mem::take(&mut self.mods.mod_entities.died) {
                self.mods.mods.entity_died(&e);
            }
            for e in std::mem::take(&mut self.mods.mod_entities.removed) {
                self.mods.mods.entity_removed(&e);
            }
            let live = std::mem::take(&mut self.mods.entity_live);
            self.mods.mods.entity_tick(self.ticks, &live);
            self.mod_items_tick(&players);
            // Capability `inter-mod`: the mods' messages to each other, at
            // the tick's end — every other hook's sends are in by now.
            self.mods.mods.inter_mod_tick();
            self.sync_mod_displays(None);
            self.return_world();
            self.refresh_mod_menus(None);
            self.mods.players = players;
            self.show_mod_output();
            self.apply_mod_block_writes();
        }

        /// Each server tick, after `block_entity_tick`: every claimed stack
        /// of every online player to the mod that claims it, held or worn
        /// (`inventory-tick`, capability `item-hooks`). A longer list than
        /// one call's is walked round-robin by the tick, so the same stacks
        /// are not cut off every tick.
        fn mod_items_tick(&mut self, players: &[ServerPlayer]) {
            let claims = self.mods.mods.item_use_claims();
            if claims.is_empty() || players.is_empty() {
                return;
            }
            let stacks = player_stacks(self, None);
            let mut lists = Vec::with_capacity(claims.len());
            for (id, items) in &claims {
                let mut held: Vec<PlayerStack> = Vec::new();
                for (player, slots) in &stacks {
                    let Some(p) = players.iter().find(|p| &p.id == player) else {
                        continue;
                    };
                    for (slot, stack) in slots.iter().enumerate() {
                        let Some(stack) = stack else {
                            continue;
                        };
                        if !items.iter().any(|i| i == stack.item.as_ref()) {
                            continue;
                        }
                        held.push(PlayerStack {
                            player: p.clone(),
                            slot: slot as u32,
                            stack: HeldStack {
                                item: stack.item.to_string(),
                                count: stack.count,
                                data: mod_stack_data(stack, id),
                            },
                        });
                    }
                }
                let len = held.len();
                if len > TICK_ITEMS {
                    held.rotate_left(self.ticks as usize % len);
                    held.truncate(TICK_ITEMS);
                }
                lists.push((id.clone(), held));
            }
            self.mods.mods.player_items_tick(self.ticks, &lists);
        }

        /// Players' command trees list the running server mods' commands
        /// after vanilla's, whenever they change (or a hub opens).
        fn send_server_mod_commands(&mut self) {
            let key = self.mods.mods.generation();
            let Some(host) = self.lan.as_mut() else {
                return;
            };
            if host.hub.mod_commands_key() == Some(key) {
                return;
            }
            let tree = command_tree(self.mods.mods.server_commands());
            host.hub.set_mod_commands(key, tree);
        }

        /// Lends the levels' chunks to capability `world` for the calls up to
        /// [`Game::return_world`], when a running server mod has it. The
        /// server's stores and the players' stacks are lent whatever the
        /// mods' capabilities: item hooks (capability `item-hooks`) change
        /// stacks beside block entities' buffers, in the same hooks.
        fn lend_world(&mut self) {
            self.lend_world_with(None);
        }

        /// [`Game::lend_world`], with the hub when it is out of `self.lan`
        /// (a connection's command): the stacks of every player on it are
        /// lent, not only those the game can reach.
        fn lend_world_with(&mut self, host: Option<&mut LanHost>) {
            self.lend_mod_entities(true);
            self.read_mod_block_entities();
            self.refresh_mod_redstone();
            let store = std::mem::take(&mut self.mods.block_entities);
            self.mods.mods.lend_block_entities(store);
            self.lend_player_items(host.as_deref());
            if !self.mods.world {
                return;
            }
            let mut dims = vec![(self.dim.id(), std::mem::take(&mut self.world.chunks))];
            for (dim, chunks) in self.levels.parked_chunks() {
                dims.push((dim.id(), std::mem::take(chunks)));
            }
            let registry = self.pack.assets.registry;
            let biomes = self.res.biomes().clone();
            let max_stacks = self
                .mods
                .max_stacks
                .get_or_insert_with(|| {
                    let items = &self.res.items().items;
                    std::sync::Arc::new(
                        items
                            .iter()
                            .map(|(k, v)| (k.to_string(), v.max_stack))
                            .collect(),
                    )
                })
                .clone();
            self.mods.mods.lend_world(Box::new(LentWorld {
                registry,
                biomes,
                dims,
                max_stacks,
            }));
        }

        /// Reads capability `block-entities`' store from the world folder,
        /// once per world.
        fn read_mod_block_entities(&mut self) {
            if self.world_root.as_os_str().is_empty()
                || self.mods.block_entities_world.as_ref() == Some(&self.world_root)
            {
                return;
            }
            self.mods.block_entities = horizon_mods::BlockEntities::load(&self.world_root);
            self.mods.block_entities_world = Some(self.world_root.clone());
        }

        /// Saves capability `block-entities`' store with the world, when it
        /// changed.
        pub(in crate::game) fn save_mod_block_entities(&mut self) {
            if self.mods.block_entities_world.as_ref() != Some(&self.world_root) {
                return;
            }
            if let Err(e) = self.mods.block_entities.save(&self.world_root) {
                log::warn!("Horizon: saving the mods' block entities: {e}");
            }
        }

        /// Puts back the chunks [`Game::lend_world`] lent.
        fn return_world(&mut self) {
            self.return_world_with(None);
        }

        /// [`Game::return_world`], with the hub when it is out of
        /// `self.lan` (a connection's command), for the players' stacks.
        fn return_world_with(&mut self, host: Option<&mut LanHost>) {
            self.return_mod_entities();
            if let Some(store) = self.mods.mods.take_block_entities() {
                self.mods.block_entities = store;
            }
            self.return_lent_player_items(host);
            let Some(view) = self.mods.mods.take_world() else {
                return;
            };
            let view: Box<dyn std::any::Any> = view;
            let Ok(lent) = view.downcast::<LentWorld>() else {
                return;
            };
            let mut dims = lent.dims;
            if let Some(i) = dims.iter().position(|(d, _)| *d == self.dim.id()) {
                self.world.chunks = dims.swap_remove(i).1;
            }
            for (dim, chunks) in self.levels.parked_chunks() {
                if let Some(i) = dims.iter().position(|(d, _)| *d == dim.id()) {
                    *chunks = dims.swap_remove(i).1;
                }
            }
        }

        /// Makes the blocks server mods set in the level in place, in order,
        /// as `/setblock` does. Those of other dimensions wait for their level.
        pub(in crate::game) fn apply_mod_block_writes(&mut self) {
            if !self.mods.drops.is_empty() {
                let dim = self.dim.id();
                let (now, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.mods.drops)
                    .into_iter()
                    .partition(|(d, _, _)| d == dim);
                self.mods.drops = later;
                for (_, pos, s) in now {
                    if let Some(stack) = self.mod_stack(&s.item, s.count, &s.components) {
                        self.pop_resource(pos, stack);
                    }
                }
            }
            if self.mods.block_writes.is_empty() {
                return;
            }
            let dim = self.dim.id();
            let (now, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.mods.block_writes)
                .into_iter()
                .partition(|(d, _)| d == dim);
            self.mods.block_writes = later;
            let registry = self.pack.assets.registry;
            let claims = self.mods.mods.block_entity_claims();
            if !claims.is_empty() {
                self.read_mod_block_entities();
            }
            for (_, b) in now {
                let state = match crate::game::render_control::parse_state(registry, &b.state) {
                    Ok(state) => state,
                    Err(e) => {
                        log::warn!("Horizon: a server mod set {}: {e}", b.state);
                        continue;
                    }
                };
                match self.place_state([b.x, b.y, b.z], state, false) {
                    Ok(_) => horizon_mods::claim(
                        &mut self.mods.block_entities,
                        &claims,
                        dim,
                        [b.x, b.y, b.z],
                        &b.state,
                    ),
                    Err(e) => log::debug!(
                        "Horizon: a server mod's block at {} {} {}: {e:?}",
                        b.x,
                        b.y,
                        b.z
                    ),
                }
            }
        }

        /// A player broke (or placed) `state` at `pos`: the player acting now.
        pub(in crate::game) fn mod_block_event(
            &mut self,
            pos: [i32; 3],
            state: BlockStateId,
            placed: bool,
        ) {
            if !self.hears_players() {
                return;
            }
            let who = self.lan_acting.map_or(Who::Local, Who::Conn);
            let state = self.state_string(state);
            self.mods.events.push(ModEvent::Block {
                who,
                pos,
                state,
                placed,
            });
        }

        /// A mod menu's items as the game's stacks, slot by slot.
        fn mod_menu_stacks(&self, rows: u32, items: &[MenuItem]) -> Vec<Option<ItemStack>> {
            let mut out = vec![None; rows as usize * 9];
            for i in items {
                let Some(slot) = out.get_mut(i.slot as usize) else {
                    continue;
                };
                *slot = self.mod_stack(&i.item, i.count, &i.components);
            }
            out
        }

        /// A mod's stack (item, count, components in item syntax) as the
        /// game's; None for an item the game doesn't know.
        fn mod_stack(&self, item: &str, count: u32, components: &str) -> Option<ItemStack> {
            Self::stack_of(self.res.items(), item, count, components)
        }
        fn stack_of(
            items: &horizon_client::items::Items,
            item: &str,
            count: u32,
            components: &str,
        ) -> Option<ItemStack> {
            if items.get(item).is_none() {
                log::debug!("Horizon: a mod shows unknown item {item}");
                return None;
            }
            let (damage, extra) = horizon_client::item_data::command_components(components)
                .map_or((0, None), |tag| {
                    horizon_client::item_data::decode_components(Some(&tag))
                });
            Some(ItemStack {
                item: item.into(),
                count,
                damage,
                extra,
            })
        }

        /// The game's stack as a mod's inventory keeps it.
        fn store_stack(stack: &ItemStack) -> Stack {
            let components = match horizon_client::item_data::encode_components(stack) {
                Some(horizon_world::nbt::Tag::Compound(map)) if !map.is_empty() => {
                    let parts: Vec<String> = map
                        .iter()
                        .map(|(k, v)| {
                            format!(
                                "{}={}",
                                k.to_string_lossy(),
                                horizon_client::snbt::to_snbt(v)
                            )
                        })
                        .collect();
                    format!("[{}]", parts.join(","))
                }
                _ => String::new(),
            };
            Stack {
                item: stack.item.to_string(),
                count: stack.count,
                components,
            }
        }

        /// What a mod's menu shows now: its items, and in its bound slots the
        /// inventory's.
        fn mod_menu_view(&self, m: &ModMenu) -> Vec<Option<ItemStack>> {
            Self::menu_view(self.res.items(), &self.mods.block_entities, m)
        }
        fn menu_view(
            items: &horizon_client::items::Items,
            store: &horizon_mods::BlockEntities,
            m: &ModMenu,
        ) -> Vec<Option<ItemStack>> {
            let mut view = m.items.clone();
            if let Some(inv) = &m.inventory {
                let key = (inv.dimension.clone(), inv.pos);
                let slots = store.items.get(&key).unwrap_or_default();
                for s in &inv.slots {
                    if let Some(v) = view.get_mut(s.slot as usize) {
                        *v = slots
                            .get(s.inventory_slot as usize)
                            .cloned()
                            .flatten()
                            .and_then(|st| {
                                Self::stack_of(items, &st.item, st.count, &st.components)
                            });
                    }
                }
            }
            view
        }

        /// Open mod menus with bound slots show their inventories as they
        /// change (a machine filling its output).
        fn refresh_mod_menus(&mut self, host: Option<&mut LanHost>) {
            if !self.mods.menus.iter().any(|m| m.inventory.is_some()) {
                return;
            }
            let Some(host) = host.or(self.lan.as_mut()) else {
                return;
            };
            for i in 0..self.mods.menus.len() {
                if self.mods.menus[i].inventory.is_none() {
                    continue;
                }
                let view = Self::menu_view(
                    self.res.items(),
                    &self.mods.block_entities,
                    &self.mods.menus[i],
                );
                let m = &mut self.mods.menus[i];
                if view != m.sent {
                    host.hub.mod_menu_content(m.conn, m.window, &view);
                    m.sent = view;
                }
            }
        }

        /// Sends the mods' displays to the players in their dimensions that
        /// don't have them yet, and drops those of mods no longer running.
        pub(in crate::game) fn sync_mod_displays(&mut self, host: Option<&mut LanHost>) {
            let Some(host) = host.or(self.lan.as_mut()) else {
                return;
            };
            Self::sync_displays(&mut self.mods.displays, &self.mods.mods, host);
        }

        /// Shows or changes a display entity (`show`, `show-item`): when it
        /// keeps its kind, dimension and position only its data changes (the
        /// client blends to it), otherwise it is remade; past
        /// [`horizon_mods::MAX_DISPLAYS`] displays of the mod the new one is
        /// dropped, as `show`'s contract says.
        fn place_display(
            displays: &mut Vec<ModDisplay>,
            next_display: &mut i32,
            mods: &horizon_mods::Mods,
            host: &mut LanHost,
            mod_id: String,
            name: String,
            dimension: String,
            pos: [f64; 3],
            kind: &'static str,
            meta: Vec<(u8, i32, Data)>,
        ) {
            let at = displays
                .iter()
                .position(|d| d.mod_id == mod_id && d.name == name);
            if let Some(i) = at {
                let d = &mut displays[i];
                if d.kind == kind && d.dimension == dimension && d.pos == pos {
                    // Only the data changes: the client blends to it.
                    for conn in &d.sent {
                        host.hub.send_to(
                            *conn,
                            &C::SetEntityData(play::SetEntityData {
                                id: d.id,
                                values: meta.clone(),
                            }),
                        );
                    }
                    d.meta = meta;
                    return;
                }
                let d = displays.remove(i);
                Self::remove_display(host, &d);
            }
            let count = displays.iter().filter(|d| d.mod_id == mod_id).count();
            if count >= horizon_mods::MAX_DISPLAYS {
                log::warn!("Horizon: mod {mod_id} shows {count} displays already");
                return;
            }
            let id = FIRST_DISPLAY_ID + *next_display;
            *next_display = (*next_display + 1) % 0x0FFF_FFFF;
            displays.push(ModDisplay {
                id,
                mod_id,
                name,
                dimension,
                pos,
                kind,
                meta,
                sent: Vec::new(),
                riding: None,
            });
            Self::sync_displays(displays, mods, host);
        }

        fn sync_displays(
            displays: &mut Vec<ModDisplay>,
            mods: &horizon_mods::Mods,
            host: &mut LanHost,
        ) {
            if displays.is_empty() {
                return;
            }
            let (keep, gone): (Vec<_>, Vec<_>) = std::mem::take(displays)
                .into_iter()
                .partition(|d| mods.runs(&d.mod_id));
            *displays = keep;
            for d in gone {
                Self::remove_display(host, &d);
            }
            for d in displays.iter_mut() {
                // A player who left, or changed dimension, has it no longer.
                d.sent.retain(|c| {
                    host.hub
                        .guests
                        .iter()
                        .any(|g| g.conn == *c && g.placed && g.dimension == d.dimension)
                });
                let Some(kind) = host.hub.entity_type_id(d.kind) else {
                    continue;
                };
                let new: Vec<u32> = host
                    .hub
                    .guests
                    .iter()
                    .filter(|g| g.placed && g.dimension == d.dimension && !d.sent.contains(&g.conn))
                    .map(|g| g.conn)
                    .collect();
                for conn in new {
                    host.hub.send_to(
                        conn,
                        &play::Clientbound::AddEntity(play::AddEntity {
                            id: d.id,
                            uuid: 0x686d_6f64_0000_0000_0000_0000_0000_0000 | d.id as u128,
                            kind,
                            position: d.pos,
                            velocity: [0.0; 3],
                            pitch: 0,
                            yaw: 0,
                            head_yaw: 0,
                            data: 0,
                        }),
                    );
                    host.hub.send_to(
                        conn,
                        &C::SetEntityData(play::SetEntityData {
                            id: d.id,
                            values: d.meta.clone(),
                        }),
                    );
                    if let Some(v) = d.riding {
                        // A guest that already tracks the mob it rides holds
                        // its passengers as they were; the display joins them
                        // now, from the hub's book (one list with the mob's
                        // own riders).
                        host.hub
                            .send_to(conn, &host.hub.riding.set_passengers(v as i32));
                    }
                    d.sent.push(conn);
                }
            }
        }

        fn remove_display(host: &LanHost, d: &ModDisplay) {
            for conn in &d.sent {
                host.hub.send_to(
                    *conn,
                    &C::RemoveEntities(play::RemoveEntities { ids: vec![d.id] }),
                );
            }
        }

        /// The connection of the player with this id, on `host`.
        fn conn_of(host: &LanHost, id: &str) -> Option<u32> {
            host.hub
                .guests
                .iter()
                .find(|g| g.placed && uuid_string(g.uuid) == id)
                .map(|g| g.conn)
        }

        /// A click in a window: a mod's menu takes it (and shows its items
        /// again), others go on to the game's menus.
        pub(in crate::game) fn lan_mod_menu_click(
            &mut self,
            host: &mut LanHost,
            conn: u32,
            p: &play::ContainerClick,
        ) -> bool {
            let Some(mi) = self
                .mods
                .menus
                .iter()
                .position(|m| m.conn == conn && m.window == p.container)
            else {
                return false;
            };
            let m = &self.mods.menus[mi];
            let (window, items) = (m.window, m.items.clone());
            let (mod_id, menu) = (m.mod_id.clone(), m.menu.clone());
            let inventory = m.inventory.clone();
            let mut bound = false;
            if let Some(inv) = &inventory {
                self.read_mod_block_entities();
                let mut view = self.mod_menu_view(&self.mods.menus[mi]);
                let mut modes = vec![ModSlot::Button; view.len()];
                for s in &inv.slots {
                    if let Some(mode) = modes.get_mut(s.slot as usize) {
                        *mode = match s.mode {
                            SlotMode::Storage => ModSlot::Storage,
                            SlotMode::Input => ModSlot::Input,
                            SlotMode::Output => ModSlot::Output,
                        };
                    }
                }
                let rules = super::super::lan_actions::item_rules(self.res.items());
                host.hub
                    .mod_menu_click(conn, &mut view, &modes, p.slot, p.button, p.input, &rules);
                // The bound slots back into the inventory.
                let key = (inv.dimension.clone(), inv.pos);
                if let Ok(slots) = self.mods.block_entities.items.slots_mut(None, &key) {
                    for s in &inv.slots {
                        if let (Some(v), Some(dst)) = (
                            view.get(s.slot as usize),
                            slots.get_mut(s.inventory_slot as usize),
                        ) {
                            *dst = v.as_ref().map(Self::store_stack);
                        }
                    }
                }
                bound =
                    usize::try_from(p.slot).is_ok_and(|s| modes.get(s) != Some(&ModSlot::Button));
            }
            // Pickup (0) and quick move (1) on the menu's own items reach the
            // mod; drags, swaps, bound slots and the inventory below don't.
            if let Ok(slot) = u32::try_from(p.slot)
                && (slot as usize) < items.len()
                && !bound
                && matches!(p.input, 0 | 1)
                && self.hears_players()
            {
                let button = u8::try_from(p.button).unwrap_or(0);
                let event =
                    ModEvent::MenuClick(Who::Conn(conn), mod_id, menu, slot, button, p.input == 1);
                self.mods.events.push(event);
            }
            let view = match self
                .mods
                .menus
                .iter()
                .find(|m| m.window == window && m.conn == conn)
            {
                Some(m) => self.mod_menu_view(m),
                None => items,
            };
            host.hub.mod_menu_content(conn, window, &view);
            if let Some(m) = self
                .mods
                .menus
                .iter_mut()
                .find(|m| m.window == window && m.conn == conn)
            {
                m.sent = view;
            }
            true
        }

        /// The client closed window `window`: true when it was a mod's menu.
        pub(in crate::game) fn lan_mod_menu_closed(&mut self, conn: u32, window: i32) -> bool {
            let Some(i) = self
                .mods
                .menus
                .iter()
                .position(|m| m.conn == conn && m.window == window)
            else {
                return false;
            };
            let m = self.mods.menus.remove(i);
            if self.hears_players() {
                let event = ModEvent::MenuClosed(Who::Conn(conn), m.mod_id, m.menu);
                self.mods.events.push(event);
            }
            true
        }

        /// A mod message a connection's player sent (`horizon_protocol::mod_net`):
        /// the mod with the id it names hears it at the tick's end. A player
        /// sends at most [`NET_PER_TICK`] a tick, and what breaks a cap or
        /// names no running mod that listens is dropped.
        pub(in crate::game) fn guest_mod_net(&mut self, conn: u32, data: &[u8]) {
            if !self.hears_players() {
                return;
            }
            let Ok(m) = horizon_protocol::mod_net::decode(data) else {
                return;
            };
            if !self.mods.mods.hears_net(&m.mod_id) {
                return;
            }
            let count = self.mods.net_counts.entry(conn).or_default();
            if *count >= NET_PER_TICK {
                return;
            }
            *count += 1;
            let event = ModEvent::NetFromClient(Who::Conn(conn), m.mod_id, m.name, m.data);
            self.mods.events.push(event);
        }

        /// The marks a mod puts on its mobs, in their saved data (`Tags`):
        /// `horizon_mod:<id>`, `horizon_tag:<name>`, and `horizon_use` for a
        /// mob that takes right clicks.
        fn entity_marks(owner: &str, tag: &str, takes_use: bool) -> Vec<String> {
            let mut marks = vec![format!("horizon_mod:{owner}"), format!("horizon_tag:{tag}")];
            if takes_use {
                marks.push("horizon_use".into());
            }
            marks
        }

        /// A mob's mod, tag and use from its saved `Tags`; None for a mob of
        /// no mod.
        fn entity_owner(raw: Option<&horizon_world::nbt::Tag>) -> Option<(String, String, bool)> {
            use horizon_world::nbt::Tag;
            let Some(Tag::List(tags)) = raw.and_then(|r| r.get("Tags")) else {
                return None;
            };
            let (mut owner, mut tag, mut takes_use) = (None, String::new(), false);
            for t in tags {
                let Tag::String(s) = t else { continue };
                let s = s.to_string_lossy();
                if let Some(id) = s.strip_prefix("horizon_mod:") {
                    owner = Some(id.to_owned());
                } else if let Some(t) = s.strip_prefix("horizon_tag:") {
                    tag = t.to_owned();
                } else if s == "horizon_use" {
                    takes_use = true;
                }
            }
            owner.map(|o| (o, tag, takes_use))
        }

        /// Looks at the level's mobs for the mods' own (capability
        /// `entities`): each mob's mark is read once, then it is followed.
        /// The mobs that started dying and those gone since the last scan are
        /// kept for the tick to tell; the live ones come back, as they stand.
        fn scan_mod_entities(&mut self) -> Vec<horizon_mods::EntityInfo> {
            use horizon_mods::EntityInfo;
            if !self.mods.mods.wants_entities() {
                return Vec::new();
            }
            let book = &mut self.mods.mod_entities;
            if book.world.as_ref() != Some(&self.world_root) {
                *book = Default::default();
                book.world = Some(self.world_root.clone());
            }
            let dimension = self.dim.id().to_owned();
            let mut present = std::collections::HashSet::new();
            let mut live = Vec::new();
            for m in &self.mobs {
                if m.removed {
                    continue;
                }
                present.insert(m.id);
                if book.plain.contains(&m.id) {
                    continue;
                }
                let kind = format!("minecraft:{}", m.kind.id.trim_start_matches("minecraft:"));
                let info = |owner: &str, tag: &str| EntityInfo {
                    id: m.id,
                    owner: owner.to_owned(),
                    kind: kind.clone(),
                    tag: tag.to_owned(),
                    dimension: dimension.clone(),
                    pos: [m.position.x, m.position.y, m.position.z],
                    yaw: m.yaw,
                    health: m.health,
                    max_health: m.attrs.max_health,
                    dying: !m.alive(),
                };
                if let std::collections::hash_map::Entry::Vacant(slot) = book.owned.entry(m.id) {
                    match Self::entity_owner(self.persist.raw.get(&m.id)) {
                        Some((owner, tag, takes_use)) if self.mods.mods.hears_entities(&owner) => {
                            slot.insert(super::OwnedEntity {
                                takes_use,
                                last: info(&owner, &tag),
                                died: false,
                            });
                        }
                        _ => {
                            book.plain.insert(m.id);
                            continue;
                        }
                    }
                }
                let Some(o) = book.owned.get_mut(&m.id) else {
                    continue;
                };
                let now = info(&o.last.owner, &o.last.tag);
                if now.dying && !o.died {
                    o.died = true;
                    book.died.push(now.clone());
                }
                o.last = now.clone();
                live.push(now);
            }
            book.plain.retain(|id| present.contains(id));
            let gone: Vec<u32> = book
                .owned
                .keys()
                .filter(|id| !present.contains(id))
                .copied()
                .collect();
            for id in gone {
                if let Some(o) = book.owned.remove(&id) {
                    book.removed.push(o.last);
                }
            }
            live
        }

        /// Lends the mods' mobs for the calls until
        /// [`Game::return_mod_entities`], when a running mod has capability
        /// `entities`. `can_spawn`: the game applies spawns right after the
        /// calls (it does not mid-tick).
        fn lend_mod_entities(&mut self, can_spawn: bool) {
            if !self.mods.mods.wants_entities() {
                return;
            }
            let live = self.scan_mod_entities();
            let kinds = self
                .mods
                .entity_kinds
                .get_or_insert_with(|| {
                    std::sync::Arc::new(self.res.mob_types().keys().cloned().collect())
                })
                .clone();
            self.mods.mods.lend_entities(horizon_mods::EntityState {
                next_id: self.next_entity + 1,
                dimension: self.dim.id().to_owned(),
                can_spawn,
                kinds,
                entities: live.clone(),
            });
            self.mods.entity_live = live;
        }

        /// Takes the lent mobs back: the ids the mods were promised are the
        /// game's now.
        fn return_mod_entities(&mut self) {
            if let Some(state) = self.mods.mods.take_entities() {
                self.next_entity = self.next_entity.max(state.next_id.saturating_sub(1));
            }
        }

        /// Capability `redstone`: keeps `mod_emitters` to what this level's
        /// blocks give out (read again when the world or dimension changes),
        /// and reads the signals around the mods' blocks once a tick.
        fn refresh_mod_redstone(&mut self) {
            let key = (self.world_root.clone(), self.dim.id().to_owned());
            if self.mods.emitters_key.as_ref() != Some(&key) {
                let dim = key.1.clone();
                self.mod_emitters = self
                    .mods
                    .block_entities
                    .redstone
                    .outputs()
                    .filter(|((d, _), _)| *d == dim)
                    .map(|((_, p), o)| ((p[0], p[1], p[2]), (o.power, o.strong)))
                    .collect();
                self.mods.emitters_key = Some(key);
            }
            if !self.mods.mods.wants_redstone() || self.mods.redstone_fed == self.ticks + 1 {
                return;
            }
            self.mods.redstone_fed = self.ticks + 1;
            let dim = self.dim.id().to_owned();
            let mut places = Vec::new();
            for id in self.mods.mods.redstone_mods() {
                for ((d, p), _) in self.mods.block_entities.of(&id) {
                    if *d == dim {
                        places.push(((d.clone(), *p), *p));
                    }
                }
            }
            for (key, p) in places {
                let at = horizon_core::position::BlockPos::new(p[0], p[1], p[2]);
                let read = self.with_level(|b, level, ticks| b.neighbor_signals(level, ticks, at));
                self.mods
                    .block_entities
                    .redstone
                    .inputs
                    .insert(key, read.map(|s| s.clamp(0, 15) as u8));
            }
        }

        /// Capability `game-events`: what changed about each player since the
        /// last tick (their health, dimension and game mode), and the weather.
        fn tell_game_events(
            &mut self,
            players: &[horizon_mods::ServerPlayer],
            before: &[horizon_mods::ServerPlayer],
        ) {
            if !self.mods.mods.wants_game_events() {
                self.mods.death_causes.clear();
                return;
            }
            for p in players {
                let Some(b) = before.iter().find(|q| q.id == p.id) else {
                    continue;
                };
                if p.dimension != b.dimension {
                    self.mods
                        .mods
                        .dimension_changed(p, &b.dimension, &p.dimension);
                }
                if p.game_mode != b.game_mode {
                    self.mods
                        .mods
                        .game_mode_changed(p, b.game_mode, p.game_mode);
                }
                if b.health > 0.0 && p.health <= 0.0 {
                    let cause = self.mods.death_causes.remove(&p.id).unwrap_or_default();
                    self.mods.mods.player_died(p, &cause);
                } else if b.health <= 0.0 && p.health > 0.0 {
                    self.mods.mods.player_respawned(p);
                } else if p.health > 0.0 && b.health - p.health > 0.001 {
                    self.mods.mods.player_hurt(p, b.health - p.health, p.health);
                }
            }
            self.mods.death_causes.clear();
            let weather = if self.world.weather.thundering {
                "thunder"
            } else if self.world.weather.raining {
                "rain"
            } else {
                "clear"
            };
            if self.mods.weather.is_some_and(|w| w != weather) {
                self.mods.mods.weather_changed(weather);
            }
            self.mods.weather = Some(weather);
        }

        /// A player died of `cause` (the death message, plain): kept for the
        /// tick's end, when the mods are told (capability `game-events`).
        /// `uuid` None is the window's own player.
        pub(in crate::game) fn mod_note_death(&mut self, uuid: Option<u128>, cause: String) {
            if !self.mods.mods.wants_game_events() {
                return;
            }
            let id = match uuid {
                Some(u) => uuid_string(u),
                None => self.player_uuid_string(),
            };
            self.mods.death_causes.insert(id, cause);
        }

        /// The acting player's blow killed mob `index` (capability
        /// `game-events`).
        pub(in crate::game) fn mod_mob_killed(&mut self, index: usize) {
            if !self.hears_players() || !self.mods.mods.wants_game_events() {
                return;
            }
            let m = &self.mobs[index];
            let kind = format!("minecraft:{}", m.kind.id.trim_start_matches("minecraft:"));
            let who = self.lan_acting.map_or(Who::Local, Who::Conn);
            self.mods.events.push(ModEvent::MobKilled(who, kind, m.id));
        }

        /// A sound name as the game writes it: `minecraft:` unless it has a
        /// namespace.
        fn sound_name(name: &str) -> String {
            if name.contains(':') {
                name.to_owned()
            } else {
                format!("minecraft:{name}")
            }
        }

        /// The packet for a particle a mod sent (capability `effects`), or
        /// None (logged) for one the game can't make: an unknown type, one
        /// that needs data the mod can't give, or a block state that does not
        /// exist.
        fn mod_particle_packet(
            registry: &horizon_world::registry::BlockRegistry,
            host: &LanHost,
            p: &horizon_mods::EffectParticle,
        ) -> Option<horizon_protocol::packets::play::Clientbound> {
            use horizon_client::particles::{ParticleOptions, ParticleType};
            use horizon_protocol::packets::play::{self, Clientbound as C};
            let name = p.name.strip_prefix("minecraft:").unwrap_or(&p.name);
            // `flash` takes data the network form here does not carry.
            let ty = ParticleType::from_name(name).filter(|_| name != "flash");
            let Some(ty) = ty else {
                log::warn!(
                    "Horizon: a mod sent particle {}, which is not one it can",
                    p.name
                );
                return None;
            };
            let rgb = |c: u32| {
                [
                    ((c >> 16) & 255) as f32 / 255.0,
                    ((c >> 8) & 255) as f32 / 255.0,
                    (c & 255) as f32 / 255.0,
                ]
            };
            let mut options = ParticleOptions::simple(ty);
            if let Some(c) = p.color {
                let [r, g, b] = rgb(c);
                options.color = [r, g, b, 1.0];
                options.to_color = [r, g, b];
            }
            if let Some(s) = p.scale {
                options.scale = s;
            }
            let state = p
                .block
                .as_deref()
                .and_then(|text| super::super::render_control::parse_state(registry, text).ok());
            let Some(particle) = Self::hub_particle(&host.hub, &options, state) else {
                log::warn!(
                    "Horizon: a mod sent particle {} without what it needs (a block, a colour)",
                    p.name
                );
                return None;
            };
            Some(C::LevelParticles(play::LevelParticles {
                particle,
                override_limiter: p.always_show,
                always_show: p.always_show,
                pos: p.pos,
                dist: p.spread,
                max_speed: [p.speed; 3],
                count: p.count as i32,
                randomization: 0,
            }))
        }

        /// A mod's block now gives out `power` (capability `redstone`): the
        /// blocks around it update, as they do for a lever, and for those it
        /// powers through when the signal is strong.
        fn apply_redstone_output(&mut self, pos: [i32; 3], power: [u8; 6], strong: bool) {
            let key = (pos[0], pos[1], pos[2]);
            let before = self.mod_emitters.get(&key).copied();
            if power == [0; 6] {
                self.mod_emitters.remove(&key);
            } else {
                self.mod_emitters.insert(key, (power, strong));
            }
            let through = strong || before.is_some_and(|(_, s)| s);
            let at = horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]);
            let state = self.world.block(pos[0], pos[1], pos[2]);
            let mut around = Vec::new();
            if through {
                for (dx, dy, dz) in [
                    (-1, 0, 0),
                    (1, 0, 0),
                    (0, -1, 0),
                    (0, 1, 0),
                    (0, 0, -1),
                    (0, 0, 1),
                ] {
                    let n = at.offset(dx, dy, dz);
                    around.push((n, self.world.block(n.x, n.y, n.z)));
                }
            }
            self.with_level(|b, level, ticks| {
                b.update_neighbors_at(level, ticks, at, b.block(state));
                for (n, s) in around {
                    b.update_neighbors_at(level, ticks, n, b.block(s));
                }
            });
            let _ = self.process_level_events();
        }

        /// Marks a mob with its mod's tags in its saved data.
        fn tag_mod_entity(&mut self, id: u32, marks: &[String]) {
            use horizon_world::nbt::{JavaString, Tag};
            let list = Tag::List(
                marks
                    .iter()
                    .map(|m| Tag::String(m.as_str().into()))
                    .collect(),
            );
            let raw = self
                .persist
                .raw
                .entry(id)
                .or_insert_with(|| Tag::Compound(Default::default()));
            if let Tag::Compound(c) = raw {
                let mut all = match c.get(&JavaString::from("Tags")) {
                    Some(Tag::List(old)) => old.clone(),
                    _ => Vec::new(),
                };
                if let Tag::List(new) = list {
                    all.extend(new);
                }
                c.insert(JavaString::from("Tags"), Tag::List(all));
            }
        }

        /// A mod's mob `id` (its owner `mod_id`), to change.
        fn mod_mob(&mut self, mod_id: &str, id: u32) -> Option<&mut horizon_client::mobs::Mob> {
            let mine = self
                .mods
                .mod_entities
                .owned
                .get(&id)
                .is_some_and(|o| o.last.owner == mod_id);
            if !mine {
                return None;
            }
            self.mobs.iter_mut().find(|m| m.id == id)
        }

        /// Applies one of the mods' mob outputs (capability `entities`).
        fn apply_entity_output(&mut self, output: Output) {
            use horizon_core::geometry::Vec3;
            match output {
                Output::SpawnEntity {
                    mod_id,
                    id,
                    pos,
                    options,
                } => {
                    if self.mobs.iter().any(|m| m.id == id) {
                        log::warn!(
                            "Horizon: mod {mod_id}'s mob {id} clashes with a mob of the game"
                        );
                        return;
                    }
                    self.next_entity = id - 1;
                    let spawned = self.spawn_mob(&options.kind, pos, true);
                    self.next_entity = self.next_entity.max(id);
                    if spawned != Some(id) {
                        log::warn!("Horizon: mod {mod_id} could not spawn {}", options.kind);
                        return;
                    }
                    let marks = Self::entity_marks(&mod_id, &options.tag, options.takes_use);
                    self.tag_mod_entity(id, &marks);
                    let dimension = self.dim.id().to_owned();
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                        m.no_ai = options.no_ai;
                        m.invisible = options.invisible;
                        m.silent = options.silent;
                        m.invulnerable_flag = options.invulnerable;
                        m.no_gravity = options.no_gravity;
                        if let Some(name) = &options.name {
                            m.custom_name = Some(name.clone());
                            m.name_visible = true;
                        }
                        if let Some(h) = options.health {
                            m.health = h.min(m.attrs.max_health);
                        }
                        let kind =
                            format!("minecraft:{}", m.kind.id.trim_start_matches("minecraft:"));
                        self.mods.mod_entities.owned.insert(
                            id,
                            super::OwnedEntity {
                                takes_use: options.takes_use,
                                last: horizon_mods::EntityInfo {
                                    id,
                                    owner: mod_id,
                                    kind,
                                    tag: options.tag,
                                    dimension,
                                    pos,
                                    yaw: m.yaw,
                                    health: m.health,
                                    max_health: m.attrs.max_health,
                                    dying: false,
                                },
                                died: false,
                            },
                        );
                    }
                }
                Output::TeleportEntity { mod_id, id, pos } => {
                    if let Some(m) = self.mod_mob(&mod_id, id) {
                        m.position = Vec3::new(pos[0], pos[1], pos[2]);
                        m.fall_distance = 0.0;
                        m.needs_sync = true;
                    }
                }
                Output::PushEntity {
                    mod_id,
                    id,
                    velocity,
                } => {
                    if let Some(m) = self.mod_mob(&mod_id, id) {
                        m.velocity = Vec3::new(
                            m.velocity.x + velocity[0],
                            m.velocity.y + velocity[1],
                            m.velocity.z + velocity[2],
                        );
                        m.needs_sync = true;
                    }
                }
                Output::SetEntityHealth { mod_id, id, health } => {
                    if let Some(m) = self.mod_mob(&mod_id, id) {
                        m.health = health.min(m.attrs.max_health);
                        m.needs_sync = true;
                    }
                }
                Output::RemoveEntity { mod_id, id } => {
                    if let Some(m) = self.mod_mob(&mod_id, id) {
                        m.removed = true;
                    }
                }
                _ => {}
            }
        }

        /// A player's blow is about to hurt mob `index`: when it is a mod's,
        /// the mod answers with the damage to deal (capability `entities`).
        pub(in crate::game) fn mod_entity_hurt(&mut self, index: usize, damage: f32) -> f32 {
            if !self.hears_players() {
                return damage;
            }
            let Some(info) = self
                .mods
                .mod_entities
                .owned
                .get(&self.mobs[index].id)
                .map(|o| o.last.clone())
            else {
                return damage;
            };
            let Some(player) = self.acting_server_player(None) else {
                return damage;
            };
            let mut host = self.lan.take();
            let answer = self.with_mod_stores(host.as_mut(), |g| {
                g.mods.mods.entity_hurt(&player, &info, damage)
            });
            self.lan = host;
            answer
        }

        /// The acting player right-clicked mob `index`: true when it is a
        /// mod's that takes the use, and the game does nothing else with
        /// the click (the mod hears it at the tick's end). A sneaking
        /// player keeps vanilla's use.
        pub(in crate::game) fn mod_entity_use(&mut self, index: usize) -> bool {
            if !self.hears_players() || self.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) {
                return false;
            }
            let id = self.mobs[index].id;
            // A click names each hand in turn: the main hand's is the one.
            if self.hand != horizon_client::first_person::Hand::Main
                || !self
                    .mods
                    .mod_entities
                    .owned
                    .get(&id)
                    .is_some_and(|o| o.takes_use)
            {
                return false;
            }
            let who = self.lan_acting.map_or(Who::Local, Who::Conn);
            self.mods.events.push(ModEvent::EntityUse(who, id));
            true
        }

        /// The acting player uses the block at `pos` with the main hand: true
        /// when a server mod takes the use of it (the mod hears it at the
        /// tick's end, and the game does nothing else with the click).
        pub(in crate::game) fn mod_block_use(&mut self, pos: [i32; 3]) -> bool {
            if !self.hears_players() {
                return false;
            }
            let claims = self.mods.mods.block_use_claims();
            if claims.is_empty() {
                return false;
            }
            let state = self.state_string(self.world.block(pos[0], pos[1], pos[2]));
            let name = horizon_mods::block_entities::block_name(&state).to_owned();
            let who = || self.lan_acting.map_or(Who::Local, Who::Conn);
            let takers: Vec<String> = claims
                .into_iter()
                .filter(|(_, blocks)| blocks.contains(&name))
                .map(|(id, _)| id)
                .collect();
            for id in &takers {
                let event = ModEvent::BlockUse(who(), id.clone(), pos, state.clone());
                self.mods.events.push(event);
            }
            !takers.is_empty()
        }

        /// A chat message a player sent.
        pub(in crate::game) fn mod_player_chat(&mut self, who: Who, text: &str) {
            if self.hears_players() {
                self.mods.events.push(ModEvent::Chat(who, text.to_owned()));
            }
        }

        /// `/line` typed by the window's own player in-frame: true when a
        /// server mod owns it.
        pub(in crate::game) fn local_server_mod_command(&mut self, line: &str) -> bool {
            if !self.hears_players() {
                return false;
            }
            let players = self.server_players(None);
            let Some(me) = players.first().cloned() else {
                return false;
            };
            let snapshot = self.mod_snapshot_with(players);
            self.mods.mods.set_snapshot(snapshot);
            self.lend_world();
            let owned = self.mods.mods.server_command(&me, line);
            self.return_world();
            self.show_mod_output();
            self.apply_mod_block_writes();
            owned
        }

        /// `/line` typed by a connection's player: true when a server mod
        /// owns it, whatever the player's permission level.
        pub(in crate::game) fn guest_server_mod_command(
            &mut self,
            host: &mut LanHost,
            conn: u32,
            line: &str,
        ) -> bool {
            if !self.hears_players() {
                return false;
            }
            let Some(g) = host.hub.guest(conn) else {
                return false;
            };
            let id = uuid_string(g.uuid);
            let players = self.server_players(Some(host));
            let Some(me) = players.iter().find(|p| p.id == id).cloned() else {
                return false;
            };
            let snapshot = self.mod_snapshot_with(players);
            self.mods.mods.set_snapshot(snapshot);
            // The hub is lent with the world: the typer's stacks are in it,
            // not in the game, so /commands reach them (capability
            // `item-hooks`).
            self.lend_world_with(Some(&mut *host));
            let owned = self.mods.mods.server_command(&me, line);
            self.return_world_with(Some(&mut *host));
            if !owned {
                return false;
            }
            self.route_mod_output(Some(host));
            self.apply_mod_block_writes();
            true
        }

        /// Sends a server mod's line to the player with this id: the
        /// window's own chat for the local player, a system message for a
        /// connection's. `host` is the hub when it is out of `self.lan`.
        pub(in crate::game) fn tell_server_player(
            &mut self,
            host: Option<&mut LanHost>,
            id: &str,
            line: Text,
        ) {
            let local = self.options.execution_role.is_client()
                && self.remote.is_none()
                && self.player.is_some();
            if local && self.player_uuid_string() == id {
                self.add_mod_chat_line(line);
                return;
            }
            let Some(host) = host.or(self.lan.as_mut()) else {
                return;
            };
            let to = host
                .hub
                .guests
                .iter()
                .find(|g| g.placed && uuid_string(g.uuid) == id);
            if let Some(conn) = to.map(|g| g.conn) {
                host.hub.send_to(
                    conn,
                    &C::SystemChat(play::SystemChat {
                        content: component_nbt(&line.component()),
                        overlay: false,
                    }),
                );
            }
        }

        /// Routes a server mod's output: `Tell` and `Reply` to their player,
        /// `Broadcast` to everyone and the log.
        pub(in crate::game) fn route_server_output(
            &mut self,
            mut host: Option<&mut LanHost>,
            output: Output,
        ) {
            let named = |name: &str, text: String| {
                let mut line = Text::literal(format!("[{name}] ")).with_color(text::GRAY);
                line.push(Text::literal(text));
                line
            };
            match output {
                Output::RedstoneOutput {
                    dimension,
                    pos,
                    power,
                    strong,
                } => {
                    if dimension == self.dim.id() {
                        self.apply_redstone_output(pos, power, strong);
                    }
                }
                Output::SpawnEntity { .. }
                | Output::TeleportEntity { .. }
                | Output::PushEntity { .. }
                | Output::SetEntityHealth { .. }
                | Output::RemoveEntity { .. } => self.apply_entity_output(output),
                Output::Tell { name, player, text } => {
                    self.tell_server_player(host, &player, named(&name, text));
                }
                Output::Reply {
                    player,
                    text,
                    error,
                } => {
                    let line = Text::literal(text);
                    let line = if error {
                        line.with_color(text::RED)
                    } else {
                        line
                    };
                    self.tell_server_player(host, &player, line);
                }
                Output::Broadcast { name, text } => {
                    let plain = format!("[{name}] {text}");
                    if self.options.dedicated.is_some() {
                        super::super::dedicated::log("INFO", &plain);
                    } else {
                        log::info!("{plain}");
                    }
                    for p in self.server_players(host.as_deref()) {
                        let line = named(&name, text.clone());
                        self.tell_server_player(host.as_deref_mut(), &p.id, line);
                    }
                }
                Output::Particle {
                    dimension,
                    particle,
                } => {
                    if dimension != self.dim.id() {
                        return;
                    }
                    let Some(host) = host.or(self.lan.as_mut()) else {
                        return;
                    };
                    if let Some(packet) =
                        Self::mod_particle_packet(self.pack.assets.registry, host, &particle)
                    {
                        let at = particle.pos;
                        // The players whose block is within 32 blocks (a
                        // particle that is always shown, within 512).
                        let reach = if particle.always_show { 512.0 } else { 32.0 };
                        for g in host.hub.guests.iter().filter(|g| host.hub.here(g)) {
                            let d: f64 = (0..3)
                                .map(|k| (g.position[k].floor() + 0.5 - at[k]).powi(2))
                                .sum();
                            if d < reach * reach {
                                g.send(&packet);
                            }
                        }
                    }
                }
                Output::Sound {
                    dimension,
                    name,
                    pos,
                    volume,
                    pitch,
                    category,
                } => {
                    if dimension != self.dim.id() {
                        return;
                    }
                    let Some(host) = host.or(self.lan.as_mut()) else {
                        return;
                    };
                    host.hub
                        .sound(&Self::sound_name(&name), pos, volume, pitch, category, None);
                }
                Output::SoundTo {
                    player,
                    name,
                    volume,
                    pitch,
                    category,
                } => {
                    let Some(host) = host.or(self.lan.as_mut()) else {
                        return;
                    };
                    let Some((conn, at)) = host
                        .hub
                        .guests
                        .iter()
                        .find(|g| g.placed && uuid_string(g.uuid) == player)
                        .map(|g| (g.conn, g.position))
                    else {
                        return;
                    };
                    let packet = host.hub.sound_packet(
                        &Self::sound_name(&name),
                        at,
                        volume,
                        pitch,
                        category,
                        self.ticks as i64,
                    );
                    host.hub.send_to(conn, &packet);
                }
                Output::NetToClient {
                    mod_id,
                    player,
                    channel,
                    data,
                } => {
                    let Some(host) = host.or(self.lan.as_mut()) else {
                        return;
                    };
                    let Some(conn) = Self::conn_of(host, &player) else {
                        return;
                    };
                    // Only Horizon clients carry them (the others would
                    // ignore the channel, but need not be sent it).
                    let horizon = host
                        .hub
                        .guests
                        .iter()
                        .any(|g| g.conn == conn && g.brand == "horizon");
                    let message = horizon_protocol::mod_net::Message {
                        mod_id,
                        name: channel,
                        data,
                    };
                    if horizon && let Ok(data) = horizon_protocol::mod_net::encode(&message) {
                        host.hub.send_to(
                            conn,
                            &C::CustomPayload(
                                horizon_protocol::packets::configuration::CustomPayload {
                                    channel: horizon_protocol::mod_net::CHANNEL.into(),
                                    data,
                                },
                            ),
                        );
                    }
                }
                Output::OpenMenu {
                    mod_id,
                    player,
                    menu,
                    title,
                    rows,
                    items,
                    inventory,
                } => {
                    let items = self.mod_menu_stacks(rows, &items);
                    self.read_mod_block_entities();
                    let Some(host) = host.or(self.lan.as_mut()) else {
                        log::warn!(
                            "Horizon: mod {mod_id}'s menus need the server thread (not in-frame yet)"
                        );
                        return;
                    };
                    let Some(conn) = Self::conn_of(host, &player) else {
                        return;
                    };
                    self.mods.menus.retain(|m| m.conn != conn);
                    let title = component_nbt(&Text::literal(title).component());
                    let Some(window) = host.hub.open_mod_menu(conn, rows, title) else {
                        return;
                    };
                    let mut m = ModMenu {
                        conn,
                        player,
                        window,
                        mod_id,
                        menu,
                        items,
                        inventory,
                        sent: Vec::new(),
                    };
                    m.sent = Self::menu_view(self.res.items(), &self.mods.block_entities, &m);
                    host.hub.mod_menu_content(conn, window, &m.sent);
                    self.mods.menus.push(m);
                }
                Output::DropItems {
                    dimension,
                    pos,
                    stacks,
                } => {
                    for s in stacks {
                        self.mods.drops.push((dimension.clone(), pos, s));
                    }
                }
                Output::SetMenuItems {
                    mod_id,
                    player,
                    menu,
                    items,
                } => {
                    let Some(i) =
                        self.mods.menus.iter().position(|m| {
                            m.player == player && m.mod_id == mod_id && m.menu == menu
                        })
                    else {
                        return;
                    };
                    let rows = (self.mods.menus[i].items.len() / 9) as u32;
                    let items = self.mod_menu_stacks(rows, &items);
                    let Some(host) = host.or(self.lan.as_mut()) else {
                        return;
                    };
                    self.mods.menus[i].items = items;
                    let view = Self::menu_view(
                        self.res.items(),
                        &self.mods.block_entities,
                        &self.mods.menus[i],
                    );
                    let m = &mut self.mods.menus[i];
                    host.hub.mod_menu_content(m.conn, m.window, &view);
                    m.sent = view;
                }
                Output::CloseMenu { mod_id, player } => {
                    let Some(i) = self
                        .mods
                        .menus
                        .iter()
                        .position(|m| m.player == player && m.mod_id == mod_id)
                    else {
                        return;
                    };
                    let m = self.mods.menus.remove(i);
                    if let Some(host) = host.or(self.lan.as_mut()) {
                        host.hub.close_window(m.conn, m.window);
                    }
                }
                Output::ShowDisplay {
                    mod_id,
                    name,
                    dimension,
                    pos,
                    shape,
                    transform,
                    interpolation,
                } => {
                    let registry = self.pack.assets.registry;
                    let (kind, block, text) = match shape {
                        DisplayShape::Block(state) => {
                            match crate::game::render_control::parse_state(registry, &state) {
                                Ok(id) => ("minecraft:block_display", Some(id.0 as i32), None),
                                Err(e) => {
                                    log::debug!("Horizon: mod {mod_id}'s display {state}: {e}");
                                    return;
                                }
                            }
                        }
                        DisplayShape::Text(t) => (
                            "minecraft:text_display",
                            None,
                            Some(component_nbt(&Text::literal(t).component())),
                        ),
                    };
                    let meta = display_meta(block, text, &transform, interpolation);
                    let Some(host) = host.or(self.lan.as_mut()) else {
                        return;
                    };
                    Self::place_display(
                        &mut self.mods.displays,
                        &mut self.mods.next_display,
                        &self.mods.mods,
                        host,
                        mod_id,
                        name,
                        dimension,
                        pos,
                        kind,
                        meta,
                    );
                }
                Output::ShowItemDisplay {
                    mod_id,
                    name,
                    dimension,
                    pos,
                    stack,
                    transform,
                    interpolation,
                } => {
                    let Some(item) = self.mod_stack(&stack.item, stack.count, &stack.components)
                    else {
                        log::debug!("Horizon: mod {mod_id} shows unknown item {}", stack.item);
                        return;
                    };
                    let Some(host) = host.or(self.lan.as_mut()) else {
                        return;
                    };
                    // An item display's data is the display's own; the stack
                    // is the hub's, with the registries' ids, and no
                    // billboard: an item display keeps vanilla's FIXED.
                    let mut meta = display_meta(None, None, &transform, interpolation);
                    meta.push((
                        23,
                        serializer::ITEM_STACK,
                        Data::Item(host.hub.stack(&Some(item))),
                    ));
                    Self::place_display(
                        &mut self.mods.displays,
                        &mut self.mods.next_display,
                        &self.mods.mods,
                        host,
                        mod_id,
                        name,
                        dimension,
                        pos,
                        "minecraft:item_display",
                        meta,
                    );
                }
                Output::MoveDisplay {
                    mod_id,
                    name,
                    pos,
                    interpolation,
                } => {
                    let Some(d) = self
                        .mods
                        .displays
                        .iter_mut()
                        .find(|d| d.mod_id == mod_id && d.name == name)
                    else {
                        // As `hide`: a display the mod has none of does nothing.
                        return;
                    };
                    d.riding = None;
                    d.pos = pos;
                    let Some(host) = host.or(self.lan.as_mut()) else {
                        return;
                    };
                    for conn in &d.sent {
                        // `teleport_duration` (index 10) is how many ticks the
                        // client walks an entity to a position sent after it;
                        // the move itself is the sync packet every entity
                        // moves by, without remaking the display.
                        host.hub.send_to(
                            *conn,
                            &C::SetEntityData(play::SetEntityData {
                                id: d.id,
                                values: vec![(
                                    10,
                                    serializer::INT,
                                    Data::Int(interpolation as i32),
                                )],
                            }),
                        );
                        host.hub.send_to(
                            *conn,
                            &C::EntityPositionSync(play::EntityPositionSync {
                                id: d.id,
                                path: play::PositionPath::Linear(pos),
                                yaw: 0.0,
                                pitch: 0.0,
                                on_ground: true,
                            }),
                        );
                    }
                }
                Output::RideDisplay {
                    mod_id,
                    name,
                    entity,
                } => {
                    let Some(i) = self
                        .mods
                        .displays
                        .iter()
                        .position(|d| d.mod_id == mod_id && d.name == name)
                    else {
                        return;
                    };
                    if self.mods.displays[i].riding == entity {
                        // Nothing changes, so nothing is sent: the hub tells
                        // the clients only when a mob's passengers do.
                        return;
                    }
                    if let Some(id) = entity {
                        let carrier = self.mobs.iter().any(|m| m.id == id && !m.removed)
                            && self.mods.displays[i].dimension == self.dim.id();
                        if !carrier {
                            log::debug!(
                                "Horizon: mod {mod_id}'s display {name} rides no mob {id} here"
                            );
                            return;
                        }
                        let riders = self
                            .mods
                            .displays
                            .iter()
                            .filter(|d| d.riding == Some(id))
                            .count();
                        if riders >= RIDERS_PER_MOB {
                            log::warn!("Horizon: mob {id} carries {riders} displays already");
                            return;
                        }
                        self.mods.displays[i].riding = Some(id);
                    } else {
                        self.mods.displays[i].riding = None;
                    }
                }
                Output::HideDisplay { mod_id, name } => {
                    let Some(i) = self
                        .mods
                        .displays
                        .iter()
                        .position(|d| d.mod_id == mod_id && d.name == name)
                    else {
                        return;
                    };
                    let d = self.mods.displays.remove(i);
                    if let Some(host) = host.or(self.lan.as_mut()) {
                        Self::remove_display(host, &d);
                    }
                }
                Output::SetBlocks {
                    name,
                    dimension,
                    blocks,
                } => {
                    let queued = &mut self.mods.block_writes;
                    queued.extend(blocks.into_iter().map(|b| (dimension.clone(), b)));
                    if queued.len() > MAX_QUEUED_WRITES {
                        let over = queued.len() - MAX_QUEUED_WRITES;
                        queued.drain(..over);
                        log::warn!(
                            "Horizon: mod {name} set more blocks than wait for their level: {over} dropped"
                        );
                    }
                }
                _ => {}
            }
        }
    }
}
