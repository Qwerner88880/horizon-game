//! Server mods in the game (`docs/mods.md`, "Server mods"): the game that
//! simulates the world (in-frame single player, the server thread, a LAN
//! host, the dedicated server) runs them. At the end of each tick it tells
//! them who joined and left, then what players said, broke and placed since
//! the last one, then ticks them; a player's command reaches them at once.
//! Their lines go to the players they name. With no server mod on, each hook
//! here is one check.
use super::{Game, lan::LanHost};
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
    pub(super) items: Vec<Option<horizon_client::items::ItemStack>>,
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
    /// `minecraft:block_display` or `minecraft:text_display`.
    pub(super) kind: &'static str,
    pub(super) meta: Vec<(u8, i32, horizon_protocol::entity_data::Value)>,
    /// The connections that have it.
    pub(super) sent: Vec<u32>,
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
}

#[cfg(feature = "mods")]
mod run {
    use super::ModDisplay;
    use super::{Game, LanHost, ModEvent, ModMenu, Who};
    use horizon_client::chunk_map::ChunkMap;
    use horizon_client::items::ItemStack;
    use horizon_mods::{DisplayShape, Transform};
    use horizon_protocol::entity_data::{Value as Data, serializer};

    /// Mods' display entities take ids from here up, past the world's.
    const FIRST_DISPLAY_ID: i32 = 0x6000_0000;

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
    }
    impl WorldView for LentWorld {
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
                    ModEvent::BlockUse(w, mod_id, pos, state) => {
                        if let Some(p) = who(&w) {
                            let dimension = p.dimension.clone();
                            self.mods
                                .mods
                                .block_used(&mod_id, &p, &dimension, pos, &state);
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
            self.mods.mods.server_tick(self.ticks);
            self.mods.mods.block_entity_tick(self.ticks);
            self.sync_mod_displays(None);
            self.return_world();
            self.mods.players = players;
            self.show_mod_output();
            self.apply_mod_block_writes();
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
        /// [`Game::return_world`], when a running server mod has it.
        fn lend_world(&mut self) {
            if !self.mods.world {
                return;
            }
            let mut dims = vec![(self.dim.id(), std::mem::take(&mut self.world.chunks))];
            for (dim, chunks) in self.levels.parked_chunks() {
                dims.push((dim.id(), std::mem::take(chunks)));
            }
            let registry = self.pack.assets.registry;
            let biomes = self.res.biomes().clone();
            self.mods.mods.lend_world(Box::new(LentWorld {
                registry,
                biomes,
                dims,
            }));
            self.read_mod_block_entities();
            let store = std::mem::take(&mut self.mods.block_entities);
            self.mods.mods.lend_block_entities(store);
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
            if let Some(store) = self.mods.mods.take_block_entities() {
                self.mods.block_entities = store;
            }
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
                if self.res.items().get(&i.item).is_none() {
                    log::debug!("Horizon: a mod menu shows unknown item {}", i.item);
                    continue;
                }
                let (damage, extra) = horizon_client::item_data::command_components(&i.components)
                    .map_or((0, None), |tag| {
                        horizon_client::item_data::decode_components(Some(&tag))
                    });
                *slot = Some(ItemStack {
                    item: i.item.as_str().into(),
                    count: i.count,
                    damage,
                    extra,
                });
            }
            out
        }

        /// Sends the mods' displays to the players in their dimensions that
        /// don't have them yet, and drops those of mods no longer running.
        pub(in crate::game) fn sync_mod_displays(&mut self, host: Option<&mut LanHost>) {
            let Some(host) = host.or(self.lan.as_mut()) else {
                return;
            };
            Self::sync_displays(&mut self.mods.displays, &self.mods.mods, host);
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
            let Some(m) = self
                .mods
                .menus
                .iter()
                .find(|m| m.conn == conn && m.window == p.container)
            else {
                return false;
            };
            let (window, items) = (m.window, m.items.clone());
            let (mod_id, menu) = (m.mod_id.clone(), m.menu.clone());
            // Pickup (0) and quick move (1) on the menu's own slots reach the
            // mod; drags, swaps and the inventory below don't.
            if let Ok(slot) = u32::try_from(p.slot)
                && (slot as usize) < items.len()
                && matches!(p.input, 0 | 1)
                && self.hears_players()
            {
                let button = u8::try_from(p.button).unwrap_or(0);
                let event =
                    ModEvent::MenuClick(Who::Conn(conn), mod_id, menu, slot, button, p.input == 1);
                self.mods.events.push(event);
            }
            host.hub.mod_menu_content(conn, window, &items);
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
            self.lend_world();
            let owned = self.mods.mods.server_command(&me, line);
            self.return_world();
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
                Output::OpenMenu {
                    mod_id,
                    player,
                    menu,
                    title,
                    rows,
                    items,
                } => {
                    let items = self.mod_menu_stacks(rows, &items);
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
                    host.hub.mod_menu_content(conn, window, &items);
                    self.mods.menus.push(ModMenu {
                        conn,
                        player,
                        window,
                        mod_id,
                        menu,
                        items,
                    });
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
                    let m = &mut self.mods.menus[i];
                    m.items = items;
                    host.hub.mod_menu_content(m.conn, m.window, &m.items);
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
                    let at = self
                        .mods
                        .displays
                        .iter()
                        .position(|d| d.mod_id == mod_id && d.name == name);
                    if let Some(i) = at {
                        let d = &mut self.mods.displays[i];
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
                        let d = self.mods.displays.remove(i);
                        Self::remove_display(host, &d);
                    }
                    let count = self
                        .mods
                        .displays
                        .iter()
                        .filter(|d| d.mod_id == mod_id)
                        .count();
                    if count >= horizon_mods::MAX_DISPLAYS {
                        log::warn!("Horizon: mod {mod_id} shows {count} displays already");
                        return;
                    }
                    let id = FIRST_DISPLAY_ID + self.mods.next_display;
                    self.mods.next_display = (self.mods.next_display + 1) % 0x0FFF_FFFF;
                    self.mods.displays.push(ModDisplay {
                        id,
                        mod_id,
                        name,
                        dimension,
                        pos,
                        kind,
                        meta,
                        sent: Vec::new(),
                    });
                    Self::sync_displays(&mut self.mods.displays, &self.mods.mods, host);
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
