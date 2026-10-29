//! A mod run with no game: `cargo horizon-mod test` (docs/mods.md, "Testing a mod").
//!
//! [`TestGame`] loads one `.hmod` from a scratch copy (its `storage` lives and
//! dies with the run) into the same sandbox the game uses, and plays the game's
//! part: the snapshot, ticks, frames, chat lines and commands, and for a
//! server mod the players joining, leaving, chatting and breaking or placing
//! blocks. [`Script`] is a
//! test written in TOML, `tests/<name>.toml` in the mod's folder: steps, each
//! saying what happens and what the mod must have shown by then.
use crate::{
    BlockPos, Clock, Corner, GameMode, Mods, Output, Panel, Player, Runs, ServerPlayer, Snapshot,
    Status,
};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

/// One mod in the sandbox, driven by a test.
pub struct TestGame {
    mods: Mods,
    dir: PathBuf,
    hmod: PathBuf,
    /// What the mod reads with `game-read`; ticks move the clock as the game does.
    pub player: Option<Player>,
    pub clock: Option<Clock>,
    /// The players on the server, for a server mod, in join order.
    pub players: Vec<ServerPlayer>,
    /// Every name that joined, for their id.
    names: Vec<String>,
    tick: u64,
    /// Capability `world`'s Overworld: the blocks a script set or the mod
    /// changed; every other block, and the other dimensions, are air. Every
    /// biome is the dimension's plainest: plains, nether wastes, the end.
    pub world: std::collections::BTreeMap<(i32, i32, i32), String>,
    /// Capability `block-entities`' store, lent to the mod with the world.
    pub block_entities: crate::BlockEntities,
    /// The players' stacks (capability `item-hooks`), by player id: 41
    /// slots each, hotbar first, armour feet to head 36..39, the off hand
    /// 40. What `set-data` writes stays there, as it stays on the stack in
    /// the game.
    pub stacks: std::collections::BTreeMap<String, Vec<Option<crate::items::Stack>>>,
}

/// The world a script's mod reads: [`TestGame::world`], copied for a step.
struct ScriptWorld(std::collections::BTreeMap<(i32, i32, i32), String>);

/// The players' stacks a script's mod reads and changes (capability
/// `item-hooks`): [`TestGame::stacks`], copied for a step, its `set-data`
/// writes carried back — the copy is taken before the step's own holds, so
/// only the writes come home.
struct ScriptItems {
    stacks: std::collections::BTreeMap<String, Vec<Option<crate::items::Stack>>>,
    written: Vec<(String, u32)>,
}

impl crate::PlayerItems for ScriptItems {
    fn stack(&self, player: &str, slot: u32, mod_id: &str) -> Option<(String, u32, Vec<u8>)> {
        let stack = self.stacks.get(player)?.get(slot as usize)?.as_ref()?;
        Some((
            stack.item.clone(),
            stack.count,
            data_of(&stack.components, mod_id),
        ))
    }
    fn set_data(
        &mut self,
        player: &str,
        slot: u32,
        mod_id: &str,
        data: &[u8],
    ) -> Result<(), String> {
        let stack = self
            .stacks
            .get_mut(player)
            .and_then(|slots| slots.get_mut(slot as usize))
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("slot {slot} of {player} holds no item"))?;
        set_data_in(&mut stack.components, mod_id, data);
        self.written.push((player.to_owned(), slot));
        Ok(())
    }
}

/// The mod's own bytes in a stack's `components`, as the harness keeps
/// them: a `key=value` pair among comma-separated ones, the bytes as
/// dot-separated numbers (`horizon:cells=104.101`). The game keeps them as
/// a byte-array component; the harness needs no full item syntax.
fn data_of(components: &str, mod_id: &str) -> Vec<u8> {
    let key = format!("horizon:{mod_id}=");
    components
        .split(',')
        .find_map(|part| {
            let bytes = part.strip_prefix(&key)?;
            Some(
                bytes
                    .split('.')
                    .filter(|b| !b.is_empty())
                    .filter_map(|b| b.parse().ok())
                    .collect::<Vec<u8>>(),
            )
        })
        .unwrap_or_default()
}

/// Replaces the mod's bytes in a stack's `components` (empty removes
/// them), as `set-data` does on the game's stacks.
fn set_data_in(components: &mut String, mod_id: &str, data: &[u8]) {
    let key = format!("horizon:{mod_id}=");
    let mut parts: Vec<String> = std::mem::take(components)
        .split(',')
        .filter(|part| !part.is_empty() && !part.starts_with(&key))
        .map(str::to_owned)
        .collect();
    if !data.is_empty() {
        let bytes: Vec<String> = data.iter().map(|b| b.to_string()).collect();
        parts.push(format!("{key}{}", bytes.join(".")));
    }
    *components = parts.join(",");
}

impl crate::WorldView for ScriptWorld {
    fn max_stack(&self, _item: &str) -> Option<u32> {
        Some(64)
    }
    fn block(&self, dimension: &str, x: i32, y: i32, z: i32) -> Option<String> {
        let here = (dimension == "minecraft:overworld")
            .then(|| self.0.get(&(x, y, z)).cloned())
            .flatten();
        Some(here.unwrap_or_else(|| "minecraft:air".into()))
    }
    fn biome(&self, dimension: &str, _x: i32, _y: i32, _z: i32) -> Option<String> {
        let biome = match dimension {
            "minecraft:overworld" => "minecraft:plains",
            "minecraft:the_nether" => "minecraft:nether_wastes",
            "minecraft:the_end" => "minecraft:the_end",
            _ => return None,
        };
        Some(biome.into())
    }
    fn has_dimension(&self, dimension: &str) -> bool {
        matches!(
            dimension,
            "minecraft:overworld" | "minecraft:the_nether" | "minecraft:the_end"
        )
    }
    fn check_state(&self, state: &str) -> Result<(), String> {
        // No registry here: the shape only (the game checks the names).
        let name = state.split('[').next().unwrap_or_default();
        let plain = |t: &str| {
            !t.is_empty()
                && t.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-/".contains(&b))
        };
        let ok = match name.split_once(':') {
            Some((ns, path)) => plain(ns) && plain(path),
            None => plain(name),
        };
        let closed = !state.contains('[') || state.ends_with(']');
        (ok && closed)
            .then_some(())
            .ok_or_else(|| format!("{state} is not a block state"))
    }
}

/// Where a script starts: standing at 0.5 64 0.5 in the Overworld, facing
/// south, full health and food, at the first tick of a new world's first morning.
pub fn default_player() -> Player {
    Player {
        x: 0.5,
        y: 64.0,
        z: 0.5,
        yaw: 0.0,
        pitch: 0.0,
        health: 20.0,
        food: 20,
        dimension: "minecraft:overworld".into(),
    }
}

impl TestGame {
    /// Starts the mod at `hmod`, as the game would with it turned on. Errs
    /// when it doesn't start, with the game's reason.
    pub fn load(hmod: &Path) -> Result<TestGame, String> {
        static RUN: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "horizon-mod-test-{}-{}",
            std::process::id(),
            RUN.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let copy = dir.join(hmod.file_name().unwrap_or("mod.hmod".as_ref()));
        let mut game = TestGame {
            mods: Mods::default(),
            hmod: copy.clone(),
            dir,
            player: Some(default_player()),
            clock: Some(Clock {
                game_time: 0,
                day_time: 0,
            }),
            players: Vec::new(),
            names: Vec::new(),
            tick: 0,
            world: Default::default(),
            block_entities: Default::default(),
            stacks: Default::default(),
        };
        copy_mod(hmod, &copy)?;
        game.start()?;
        Ok(game)
    }

    fn start(&mut self) -> Result<(), String> {
        let recipes = machine_recipes_of(&self.hmod);
        let multiblocks = multiblocks_of(&self.hmod);
        self.mods = Mods::load_for(
            Runs::Both,
            std::slice::from_ref(&self.hmod),
            |_| true,
            |_| false,
            recipes,
            multiblocks,
        );
        match &self.mods.entries()[0].status {
            Status::Failed(reason) => Err(format!("the mod didn't start: {reason}")),
            _ if !self.mods.is_running() => Err("the mod has no code to run".into()),
            _ => Ok(()),
        }
    }

    /// Stops the mod and starts it again, as a rebuild does in the game: its
    /// memory starts over, its `storage` stays.
    pub fn reload(&mut self) -> Result<(), String> {
        self.mods = Mods::default();
        self.start()
    }

    fn snapshot(&mut self) {
        self.mods.set_snapshot(Snapshot {
            player: self.player.clone(),
            clock: self.clock,
            players: self.players.clone(),
            // The GUI of a 1280 x 720 window at scale 3, as the trial run's.
            gui_size: (427.0, 240.0),
            ..Snapshot::default()
        });
    }
    /// Whether the mod is a server mod.
    pub fn server(&self) -> bool {
        self.mods.runs_server()
    }

    /// `n` game ticks; each moves the clock one tick on, as the game does.
    pub fn tick(&mut self, n: u64) {
        for _ in 0..n {
            self.tick += 1;
            if let Some(clock) = &mut self.clock {
                clock.game_time += 1;
                clock.day_time += 1;
            }
            self.snapshot();
            self.mods.tick(self.tick);
            self.mods.server_tick(self.tick);
            self.mods.block_entity_tick(self.tick);
            self.items_tick();
        }
    }
    /// Each tick's `inventory-tick` (capability `item-hooks`): every
    /// claimed stack of every player on, to the mod that claims it, at most
    /// [`crate::TICK_ITEMS`] a mod — a longer list walked round-robin by
    /// the tick, as the game does.
    fn items_tick(&mut self) {
        let claims = self.mods.item_use_claims();
        if claims.is_empty() || self.players.is_empty() {
            return;
        }
        let mut lists = Vec::with_capacity(claims.len());
        for (id, items) in &claims {
            let mut held: Vec<crate::PlayerStack> = Vec::new();
            for player in &self.players {
                for (slot, stack) in self
                    .stacks
                    .get(&player.id)
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    let Some(stack) = stack else {
                        continue;
                    };
                    if !items.iter().any(|i| i == &stack.item) {
                        continue;
                    }
                    held.push(crate::PlayerStack {
                        player: player.clone(),
                        slot: slot as u32,
                        stack: crate::HeldStack {
                            item: stack.item.clone(),
                            count: stack.count,
                            data: data_of(&stack.components, id),
                        },
                    });
                }
            }
            let len = held.len();
            if len > crate::TICK_ITEMS {
                held.rotate_left(self.tick as usize % len);
                held.truncate(crate::TICK_ITEMS);
            }
            lists.push((id.clone(), held));
        }
        self.mods.player_items_tick(self.tick, &lists);
    }
    /// `n` frames, 1/60 s apart.
    pub fn frames(&mut self, n: u64) {
        self.snapshot();
        for _ in 0..n {
            self.mods.frame(1.0 / 60.0);
        }
    }
    /// A chat line the player receives.
    pub fn chat(&mut self, line: &str) {
        self.snapshot();
        self.mods.chat(line);
    }
    /// `/line` typed by the player (without the slash); false when the mod
    /// owns no such command.
    pub fn command(&mut self, line: &str) -> bool {
        self.snapshot();
        self.mods.command(line)
    }

    /// A player named `name` joins the server, standing where a script starts
    /// in survival. Their id is `00000000-0000-4000-8000-000000000001` for
    /// the first name that joined, `...0002` for the second, and so on.
    pub fn join(&mut self, name: &str) -> ServerPlayer {
        let n = match self.names.iter().position(|n| n == name) {
            Some(i) => i + 1,
            None => {
                self.names.push(name.to_owned());
                self.names.len()
            }
        };
        let p = default_player();
        let player = ServerPlayer {
            id: format!("00000000-0000-4000-8000-{n:012x}"),
            name: name.to_owned(),
            x: p.x,
            y: p.y,
            z: p.z,
            dimension: p.dimension,
            game_mode: GameMode::Survival,
            health: p.health,
        };
        self.players.push(player.clone());
        self.stacks
            .entry(player.id.clone())
            .or_default()
            .resize(41, None);
        self.snapshot();
        self.mods.player_joined(&player);
        player
    }
    /// The player named `name` leaves; false when no such player is on.
    pub fn leave(&mut self, name: &str) -> bool {
        let Some(i) = self.players.iter().position(|p| p.name == name) else {
            return false;
        };
        let player = self.players.remove(i);
        self.stacks.remove(&player.id);
        self.snapshot();
        self.mods.player_left(&player);
        true
    }
    /// A chat message `player` sends, for a server mod.
    pub fn player_chat(&mut self, player: &ServerPlayer, line: &str) {
        self.snapshot();
        self.mods.player_chat(player, line);
    }
    /// `/line` typed by `player`, for a server mod; false when the mod owns
    /// no such command.
    pub fn player_command(&mut self, player: &ServerPlayer, line: &str) -> bool {
        self.snapshot();
        self.mods.server_command(player, line)
    }
    /// `player` right-clicks `state` at `pos`: a mod taking the use of
    /// that block hears it (`block-use-events`).
    pub fn use_block(&mut self, player: &ServerPlayer, pos: BlockPos, state: &str) {
        self.snapshot();
        let name = crate::block_entities::block_name(state).to_owned();
        for (id, blocks) in self.mods.block_use_claims() {
            if blocks.contains(&name) {
                let at = [pos.x, pos.y, pos.z];
                self.mods
                    .block_used(&id, player, &player.dimension, at, state);
            }
        }
    }
    /// `player` holds `count` of `item` in `slot` (the main hand is slot
    /// 0, armour feet to head 36..39), with no data of any mod's on it.
    pub fn hold(&mut self, player: &ServerPlayer, item: &str, count: u32, slot: u32) {
        self.stacks
            .entry(player.id.clone())
            .or_default()
            .resize(41, None);
        if let Some(s) = self
            .stacks
            .get_mut(&player.id)
            .and_then(|s| s.get_mut(slot as usize))
        {
            *s = Some(crate::items::Stack {
                item: item.to_owned(),
                count,
                components: String::new(),
            });
        }
    }
    /// `player` uses (right-clicks) the main hand's stack, which must be
    /// `count` of `item` (capability `item-hooks`): every mod claiming the
    /// item hears `used`, or `used-on-block` on `state` at `pos`.
    pub fn use_item(
        &mut self,
        player: &ServerPlayer,
        item: &str,
        count: u32,
        on: Option<(BlockPos, &str)>,
    ) -> Result<(), String> {
        self.snapshot();
        let stack = self.held(player, item, count)?;
        let data = |id: &str| data_of(&stack.components, id);
        for (id, items) in self.mods.item_use_claims() {
            if !items.iter().any(|i| i == &stack.item) {
                continue;
            }
            let held = crate::HeldStack {
                item: stack.item.clone(),
                count: stack.count,
                data: data(&id),
            };
            let block = on.map(|(pos, state)| ([pos.x, pos.y, pos.z], state.to_owned()));
            self.mods.item_used(
                &id,
                player,
                held,
                block.as_ref().map(|(p, s)| (*p, s.as_str())),
            );
        }
        Ok(())
    }
    /// `player`'s blow with the main hand's stack (which must be `count` of
    /// `item`) on an entity of `kind` at `health` health, for `damage`:
    /// the claiming mods' answers, chained (capability `item-hooks`).
    pub fn attack_entity(
        &mut self,
        player: &ServerPlayer,
        item: &str,
        count: u32,
        kind: &str,
        health: f32,
        damage: f32,
    ) -> Result<f32, String> {
        self.snapshot();
        let stack = self.held(player, item, count)?;
        let entity = crate::ItemEntity {
            id: 1,
            kind: kind.to_owned(),
            health,
        };
        let mut damage = damage;
        for (id, items) in self.mods.item_use_claims() {
            if !items.iter().any(|i| i == &stack.item) {
                continue;
            }
            let held = crate::HeldStack {
                item: stack.item.clone(),
                count: stack.count,
                data: data_of(&stack.components, &id),
            };
            damage = self
                .mods
                .item_hurt_entity(&id, player, held, &entity, damage);
        }
        Ok(damage)
    }
    /// `player` is hurt for `damage` of `source`: the claiming mods'
    /// armour takes its share, and what it leaves is the answer (capability
    /// `item-hooks`).
    pub fn hurt_player(&mut self, player: &ServerPlayer, source: &str, damage: f32) -> f32 {
        self.snapshot();
        self.mods.item_hurt(player, damage, source)
    }
    /// The main hand's stack, when it is `count` of `item`: slot 0.
    fn held(
        &self,
        player: &ServerPlayer,
        item: &str,
        count: u32,
    ) -> Result<crate::items::Stack, String> {
        match self
            .stacks
            .get(&player.id)
            .and_then(|s| s.first())
            .cloned()
            .flatten()
        {
            Some(s) if s.item == item && s.count == count => Ok(s),
            _ => Err(format!(
                "{} holds no {} {} in the main hand (hold = {{ item = \"{}\", count = {} }})",
                player.name, count, item, item, count
            )),
        }
    }
    /// The client half sends `data` on `channel` (capability `network`): a
    /// server mod hears it from `player`.
    pub fn net_from_client(&mut self, player: &ServerPlayer, channel: &str, data: &[u8]) {
        self.snapshot();
        if let Some(id) = self.mod_id() {
            self.mods.net_client_sent(&id, player, channel, data);
        }
    }
    /// The server half sends `data` on `channel`: a client mod hears it.
    pub fn net_from_server(&mut self, channel: &str, data: &[u8]) {
        self.snapshot();
        if let Some(id) = self.mod_id() {
            self.mods.net_server_sent(&id, channel, data);
        }
    }
    /// The player picks `row` of the mod's widget `id` (a button is row 0):
    /// a client mod's `ui-events::select`.
    pub fn ui_select(&mut self, id: u32, row: u32) {
        self.snapshot();
        self.mods.ui_select(0, id, row);
    }
    /// The player leaves the mod's screen (Esc or Done).
    pub fn ui_closed(&mut self) {
        self.snapshot();
        self.mods.ui_closed(0);
    }
    /// The mod's screen as lines: `# title`, then each widget (text as is,
    /// `[label]` for a button, `item: name` for an item, `rows` for a
    /// list), `None` while no screen is open.
    pub fn screen_lines(&self) -> Option<Vec<String>> {
        use crate::UiWidget;
        let screen = self.mods.screen(0)?;
        let mut lines = vec![format!("# {}", screen.title)];
        for w in &screen.widgets {
            lines.push(match w {
                UiWidget::Text(t) => t.text.clone(),
                UiWidget::Button(b) => format!("[{}]", b.label),
                UiWidget::Item(i) => format!("{}: {}", i.item, i.name),
                UiWidget::List(l) => l.rows.join(" | "),
            });
        }
        Some(lines)
    }
    fn mod_id(&self) -> Option<String> {
        self.mods.entries()[0]
            .manifest
            .as_ref()
            .map(|m| m.id.clone())
    }
    /// `player` clicks slot `slot` of the mod's menu `menu` (capability
    /// `menus`).
    pub fn menu_click(
        &mut self,
        player: &ServerPlayer,
        menu: &str,
        slot: u32,
        button: u8,
        shift: bool,
    ) {
        self.snapshot();
        let id = self.mods.entries()[0]
            .manifest
            .as_ref()
            .map(|m| m.id.clone());
        if let Some(id) = id {
            self.mods
                .menu_clicked(&id, player, menu, slot, button, shift);
        }
    }
    /// `player` breaks (or places) `state` at `pos`.
    pub fn block(&mut self, player: &ServerPlayer, pos: BlockPos, state: &str, placed: bool) {
        self.snapshot();
        if player.dimension == "minecraft:overworld" {
            let at = (pos.x, pos.y, pos.z);
            match placed {
                true => self.world.insert(at, state.to_owned()),
                false => self.world.remove(&at),
            };
        }
        if placed {
            let at = [pos.x, pos.y, pos.z];
            self.mods.block_entity_placed(&player.dimension, at, state);
            self.mods.block_placed(player, pos, state);
        } else {
            self.mods.block_broken(player, pos, state);
        }
    }

    /// The scratch copy the mod runs from; its `storage` is beside it.
    pub fn hmod(&self) -> &Path {
        &self.hmod
    }
    /// The commands the mod registered.
    pub fn commands(&self) -> Vec<String> {
        let server = self.mods.server_commands().map(|c| c.name.as_str());
        self.mods
            .commands()
            .chain(server)
            .map(str::to_owned)
            .collect()
    }
    pub fn panel(&self) -> Option<&Panel> {
        self.mods.panels().next()
    }
    pub fn status(&self) -> &Status {
        &self.mods.entries()[0].status
    }
    /// What the mod showed since the last call.
    pub fn drain(&mut self) -> Vec<Output> {
        self.mods.drain()
    }
}

impl Drop for TestGame {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The machine recipes in the mod at `hmod` (capability `machine-recipes`),
/// as the game builds them for its server mods: the package's own
/// `data/<id>/recipe/` files, read before the mod starts. A test's
/// ingredients are literal, so tags stay unexpanded.
fn machine_recipes_of(
    hmod: &Path,
) -> Option<std::sync::Arc<crate::machine_recipes::MachineRecipes>> {
    let package = crate::Package::read(hmod).ok()?;
    let m = &package.manifest;
    if m.side != crate::Side::Server || !m.capabilities.contains(&crate::Capability::MachineRecipes)
    {
        return None;
    }
    let files = crate::machine_recipes::files_in_package(hmod, &m.id);
    Some(std::sync::Arc::new(crate::machine_recipes::build(
        &files,
        &|_| Vec::new(),
    )))
}

/// The structures in the mod at `hmod` (capability `multiblocks`), as the
/// game builds them for its server mods: the package's own
/// `data/<id>/multiblock/` files, read before the mod starts.
fn multiblocks_of(hmod: &Path) -> Option<std::sync::Arc<crate::multiblocks::Templates>> {
    let package = crate::Package::read(hmod).ok()?;
    let m = &package.manifest;
    if m.side != crate::Side::Server || !m.capabilities.contains(&crate::Capability::Multiblocks) {
        return None;
    }
    let files = crate::multiblocks::files_in_package(hmod, &m.id);
    Some(std::sync::Arc::new(crate::multiblocks::build(&files)))
}

/// A `.hmod` file, or a mod folder (`horizon-mod.toml` + `mod.wasm`).
fn copy_mod(from: &Path, to: &Path) -> Result<(), String> {
    let err = |e: std::io::Error| format!("{}: {e}", from.display());
    if !from.is_dir() {
        return std::fs::copy(from, to).map(drop).map_err(err);
    }
    std::fs::create_dir_all(to).map_err(err)?;
    for entry in std::fs::read_dir(from).map_err(err)? {
        let entry = entry.map_err(err)?;
        if entry.file_type().map_err(err)?.is_file() {
            std::fs::copy(entry.path(), to.join(entry.file_name())).map_err(err)?;
        }
    }
    Ok(())
}

/// A test in TOML: where a client mod's player starts, then steps run in order.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Script {
    #[serde(default)]
    pub player: PlayerSet,
    #[serde(default)]
    pub clock: ClockSet,
    #[serde(default, rename = "step")]
    pub steps: Vec<Step>,
}

/// Fields of the player to change; the others keep their values. A server
/// mod's players have no `yaw`, `pitch` or `food`, and a client mod's player
/// no `game-mode`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct PlayerSet {
    x: Option<f64>,
    y: Option<f64>,
    z: Option<f64>,
    yaw: Option<f32>,
    pitch: Option<f32>,
    health: Option<f32>,
    food: Option<u32>,
    dimension: Option<String>,
    game_mode: Option<Mode>,
}

/// `players.game-mode` as a script spells it.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Mode {
    Survival,
    Creative,
    Adventure,
    Spectator,
}
impl From<Mode> for GameMode {
    fn from(m: Mode) -> GameMode {
        match m {
            Mode::Survival => GameMode::Survival,
            Mode::Creative => GameMode::Creative,
            Mode::Adventure => GameMode::Adventure,
            Mode::Spectator => GameMode::Spectator,
        }
    }
}

/// A press of one of the mod's widgets (`ui-events::select`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectSet {
    id: u32,
    #[serde(default)]
    row: u32,
}

/// A message of the other half of the mod: `data` is text, sent as its bytes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetSet {
    channel: String,
    #[serde(default)]
    data: String,
}

/// A block a player breaks or places.
/// A click in the mod's menu (capability `menus`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClickSet {
    menu: String,
    slot: u32,
    #[serde(default)]
    button: u8,
    #[serde(default)]
    shift: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockSet {
    x: i32,
    y: i32,
    z: i32,
    state: String,
}

/// A stack a player is given to hold (capability `item-hooks`): the main
/// hand is slot 0, armour feet to head 36..39, the off hand 40.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldSet {
    item: String,
    #[serde(default = "one")]
    count: u32,
    #[serde(default)]
    slot: Option<u32>,
}

/// The item a player uses (capability `item-hooks`): the main hand's
/// stack, right-clicked into the air or on `on`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UseItemSet {
    item: String,
    #[serde(default = "one")]
    count: u32,
    #[serde(default)]
    on: Option<BlockSet>,
}

/// A player's blow with the main hand's item (capability `item-hooks`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackSet {
    /// The item struck with, as the main hand holds it.
    item: String,
    #[serde(default = "one")]
    count: u32,
    kind: String,
    health: f32,
    damage: f32,
}

/// A player hurt while wearing a claimed item (capability `item-hooks`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HurtSet {
    amount: f32,
    #[serde(default)]
    source: Option<String>,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ClockSet {
    game_time: Option<u64>,
    day_time: Option<u64>,
}

/// One step. Its parts happen in this order: `reload`, `title-screen`,
/// `blocks`, `join`, `player`, `clock`, `hold`, `ticks`, `frames`, `chat`,
/// `command`, `break`, `place`, `leave`; then the blocks the mod set are
/// made, and `expect` is checked against what the mod showed and changed
/// during the step. For a server mod, `player`, `chat`, `command`, `break`
/// and `place` are the doing of the player named by `as`, or of the first
/// player on; so are `hold`, `use`, `use-item`, `attack` and `hurt`, which
/// run in that order between the `chat` and `command` parts.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Step {
    #[serde(default)]
    reload: bool,
    /// Leave the world: the mod reads no player and no clock until a step sets them.
    #[serde(default)]
    title_screen: bool,
    player: Option<PlayerSet>,
    clock: Option<ClockSet>,
    #[serde(default)]
    ticks: u64,
    #[serde(default)]
    frames: u64,
    chat: Option<String>,
    command: Option<String>,
    /// Server mods: a player joins, leaves, breaks or places a block.
    join: Option<String>,
    leave: Option<String>,
    #[serde(rename = "as")]
    actor: Option<String>,
    #[serde(rename = "break")]
    broken: Option<BlockSet>,
    place: Option<BlockSet>,
    /// Server mods: the player right-clicks a block, then clicks the mod's
    /// menu, before `command`.
    #[serde(rename = "use")]
    used: Option<BlockSet>,
    click: Option<ClickSet>,
    /// Server mods: the player holds a stack (capability `item-hooks`),
    /// uses the main hand's item, strikes an entity with it and is hurt.
    hold: Option<HoldSet>,
    use_item: Option<UseItemSet>,
    attack: Option<AttackSet>,
    hurt: Option<HurtSet>,
    /// A message of the other half of the mod (capability `network`): the
    /// player's client half for a server mod, the server half for a client
    /// mod. Before `command`.
    net: Option<NetSet>,
    /// Client mods: the player presses a widget of the mod's screen, or
    /// leaves the screen. After `net`, before `command`.
    select: Option<SelectSet>,
    #[serde(default)]
    ui_close: bool,
    /// Blocks of the Overworld before the rest of the step (capability
    /// `world`); every other block is air.
    #[serde(default)]
    blocks: Vec<BlockSet>,
    #[serde(default)]
    expect: Expect,
}

/// What a step checks; a part left out is not checked.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Expect {
    /// The commands the mod owns.
    commands: Option<Vec<String>>,
    /// Every chat line shown during the step, in order.
    chat: Option<Vec<String>>,
    /// The command's feedback, and its red error.
    feedback: Option<String>,
    error: Option<String>,
    /// Every message the mod sent (capability `network`) as `channel=text`,
    /// for a server mod `Name: channel=text` (the bytes as UTF-8, lossily).
    net: Option<Vec<String>>,
    /// The mod's screen after the step, as [`TestGame::screen_lines`]
    /// shows it; `[]` for no screen open.
    screen: Option<Vec<String>>,
    /// Server mods: every `players.tell` as `"Name: text"`, and every
    /// `players.broadcast`, in order.
    told: Option<Vec<String>>,
    broadcast: Option<Vec<String>>,
    /// The panel's lines (`[]`: no panel) and its corner.
    panel: Option<Vec<String>>,
    corner: Option<String>,
    /// Blocks of the Overworld after the step, the mod's changes made
    /// (capability `world`).
    blocks: Option<Vec<BlockSet>>,
    /// The mod is turned off during this step, for a reason holding this text.
    /// Without it, a mod that stops fails the test.
    stopped: Option<String>,
}

impl Script {
    pub fn parse(text: &str) -> Result<Script, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// Runs every step on `game`; the error names the first step that failed.
    pub fn run(&self, game: &mut TestGame) -> Result<(), String> {
        if let Some(p) = &mut game.player {
            self.player.apply(p);
        }
        if let Some(c) = &mut game.clock {
            self.clock.apply(c);
        }
        for (i, step) in self.steps.iter().enumerate() {
            step.run(game)
                .map_err(|e| format!("step {} ({}): {e}", i + 1, step.describe()))?;
        }
        Ok(())
    }
}

impl PlayerSet {
    fn apply(&self, p: &mut Player) {
        let set = |to: &mut f64, v: Option<f64>| *to = v.unwrap_or(*to);
        set(&mut p.x, self.x);
        set(&mut p.y, self.y);
        set(&mut p.z, self.z);
        p.yaw = self.yaw.unwrap_or(p.yaw);
        p.pitch = self.pitch.unwrap_or(p.pitch);
        p.health = self.health.unwrap_or(p.health);
        p.food = self.food.unwrap_or(p.food);
        if let Some(d) = &self.dimension {
            p.dimension.clone_from(d);
        }
    }
    fn apply_server(&self, p: &mut ServerPlayer) {
        let set = |to: &mut f64, v: Option<f64>| *to = v.unwrap_or(*to);
        set(&mut p.x, self.x);
        set(&mut p.y, self.y);
        set(&mut p.z, self.z);
        p.health = self.health.unwrap_or(p.health);
        p.game_mode = self.game_mode.map_or(p.game_mode, GameMode::from);
        if let Some(d) = &self.dimension {
            p.dimension.clone_from(d);
        }
    }
}

impl ClockSet {
    fn apply(&self, c: &mut Clock) {
        c.game_time = self.game_time.unwrap_or(c.game_time);
        c.day_time = self.day_time.unwrap_or(c.day_time);
    }
}

impl Step {
    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.reload {
            parts.push("reload".to_string());
        }
        let count = |n: u64, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
        if self.ticks > 0 {
            parts.push(count(self.ticks, "tick"));
        }
        if self.frames > 0 {
            parts.push(count(self.frames, "frame"));
        }
        if let Some(line) = &self.chat {
            parts.push(format!("chat {line:?}"));
        }
        if let Some(line) = &self.command {
            parts.push(format!("/{line}"));
        }
        if let Some(n) = &self.net {
            parts.push(format!("net {}", n.channel));
        }
        if let Some(s) = &self.select {
            parts.push(format!("select widget {}", s.id));
        }
        if self.ui_close {
            parts.push("closes the screen".to_string());
        }
        if let Some(hold) = &self.hold {
            parts.push(format!("holds {} x{}", hold.item, hold.count));
        }
        if let Some(use_item) = &self.use_item {
            parts.push(format!("uses {}", use_item.item));
        }
        if let Some(attack) = &self.attack {
            parts.push(format!(
                "strikes a {} for {:.1}",
                attack.kind, attack.damage
            ));
        }
        if self.hurt.is_some() {
            parts.push("is hurt".to_string());
        }
        if let Some(name) = &self.join {
            parts.insert(0, format!("{name} joins"));
        }
        for (what, b) in [("breaks", &self.broken), ("places", &self.place)] {
            if let Some(b) = b {
                parts.push(format!("{what} {} at {} {} {}", b.state, b.x, b.y, b.z));
            }
        }
        if let Some(name) = &self.leave {
            parts.push(format!("{name} leaves"));
        }
        if parts.is_empty() {
            parts.push("checks".into());
        }
        parts.join(", ")
    }

    fn run(&self, game: &mut TestGame) -> Result<(), String> {
        if self.reload {
            game.reload()?;
        }
        if self.title_screen {
            game.player = None;
            game.clock = None;
        }
        for b in &self.blocks {
            game.world.insert((b.x, b.y, b.z), b.state.clone());
        }
        game.mods
            .lend_world(Box::new(ScriptWorld(game.world.clone())));
        let store = std::mem::take(&mut game.block_entities);
        game.mods.lend_block_entities(store);
        game.mods.lend_player_items(Box::new(ScriptItems {
            stacks: game.stacks.clone(),
            written: Vec::new(),
        }));
        if let Some(name) = &self.join {
            game.join(name);
        }
        let server = game.server();
        let actor = |game: &TestGame| match &self.actor {
            Some(name) => game
                .players
                .iter()
                .position(|p| p.name == *name)
                .ok_or_else(|| format!("{name} is not on the server")),
            None => (!game.players.is_empty())
                .then_some(0)
                .ok_or_else(|| "no player is on the server: add join = \"Name\"".to_string()),
        };
        let acts = self.player.is_some()
            || self.chat.is_some()
            || self.command.is_some()
            || self.broken.is_some()
            || self.place.is_some()
            || self.click.is_some()
            || (self.net.is_some() && server)
            || self.used.is_some()
            || self.hold.is_some()
            || self.use_item.is_some()
            || self.attack.is_some()
            || self.hurt.is_some();
        let actor = if server && acts {
            Some(actor(game)?)
        } else {
            None
        };
        if let Some(set) = &self.player {
            match actor {
                Some(i) => set.apply_server(&mut game.players[i]),
                None => set.apply(game.player.get_or_insert_with(default_player)),
            }
        }
        if let Some(set) = &self.clock {
            set.apply(game.clock.get_or_insert(Clock {
                game_time: 0,
                day_time: 0,
            }));
        }
        if let (Some(hold), Some(i)) = (&self.hold, actor) {
            let player = game.players[i].clone();
            game.hold(&player, &hold.item, hold.count, hold.slot.unwrap_or(0));
        }
        game.tick(self.ticks);
        game.frames(self.frames);
        let who = actor.map(|i| game.players[i].clone());
        if let Some(line) = &self.chat {
            match &who {
                Some(p) => game.player_chat(p, line),
                None => game.chat(line),
            }
        }
        if let (Some(b), Some(p)) = (&self.used, &who) {
            let pos = BlockPos {
                x: b.x,
                y: b.y,
                z: b.z,
            };
            game.use_block(p, pos, &b.state);
        }
        if let (Some(use_item), Some(p)) = (&self.use_item, &who) {
            let on = use_item.on.as_ref().map(|b| {
                (
                    BlockPos {
                        x: b.x,
                        y: b.y,
                        z: b.z,
                    },
                    b.state.as_str(),
                )
            });
            game.use_item(p, &use_item.item, use_item.count, on)?;
        }
        if let (Some(attack), Some(p)) = (&self.attack, &who) {
            game.attack_entity(
                p,
                &attack.item,
                attack.count,
                &attack.kind,
                attack.health,
                attack.damage,
            )?;
        }
        if let (Some(hurt), Some(p)) = (&self.hurt, &who) {
            let source = hurt
                .source
                .clone()
                .unwrap_or_else(|| "minecraft:generic".into());
            game.hurt_player(p, &source, hurt.amount);
        }
        if let (Some(c), Some(p)) = (&self.click, &who) {
            game.menu_click(p, &c.menu, c.slot, c.button, c.shift);
        }
        if let Some(n) = &self.net {
            match &who {
                Some(p) => game.net_from_client(p, &n.channel, n.data.as_bytes()),
                None => game.net_from_server(&n.channel, n.data.as_bytes()),
            }
        }
        if let Some(s) = &self.select {
            game.ui_select(s.id, s.row);
        }
        if self.ui_close {
            game.ui_closed();
        }
        let owned = self.command.as_ref().map(|line| match &who {
            Some(p) => game.player_command(p, line),
            None => game.command(line),
        });
        for (b, placed) in [(&self.broken, false), (&self.place, true)] {
            if let (Some(b), Some(p)) = (b, &who) {
                let pos = BlockPos {
                    x: b.x,
                    y: b.y,
                    z: b.z,
                };
                game.block(p, pos, &b.state, placed);
            }
        }
        if let Some(name) = &self.leave
            && !game.leave(name)
        {
            return Err(format!("{name} is not on the server"));
        }
        game.mods.take_world();
        if let Some(store) = game.mods.take_block_entities() {
            game.block_entities = store;
        }
        // The stacks the mod wrote its bytes on keep them: the copy was
        // taken before the step's own holds, so only the writes come home,
        // onto whatever the step left in the slot.
        if let Some(view) = game.mods.take_player_items() {
            let view: Box<dyn std::any::Any> = view;
            if let Ok(mut items) = view.downcast::<ScriptItems>() {
                for (player, slot) in std::mem::take(&mut items.written) {
                    let written = items
                        .stacks
                        .get(&player)
                        .and_then(|slots| slots.get(slot as usize))
                        .cloned()
                        .flatten();
                    if let Some(written) = written
                        && let Some(held) = game
                            .stacks
                            .get_mut(&player)
                            .and_then(|slots| slots.get_mut(slot as usize))
                        && held.is_some()
                    {
                        *held = Some(written);
                    }
                }
            }
        }
        let out = game.drain();
        let claims = game.mods.block_entity_claims();
        for o in &out {
            if let Output::SetBlocks {
                dimension, blocks, ..
            } = o
                && dimension == "minecraft:overworld"
            {
                for b in blocks {
                    game.world.insert((b.x, b.y, b.z), b.state.clone());
                    let at = [b.x, b.y, b.z];
                    crate::claim(&mut game.block_entities, &claims, dimension, at, &b.state);
                }
            }
        }
        let stopped = out.iter().find_map(|o| match o {
            Output::Stopped { reason, .. } => Some(reason.as_str()),
            _ => None,
        });
        let e = &self.expect;
        match (stopped, &e.stopped) {
            (Some(reason), Some(want)) if !reason.contains(want.as_str()) => {
                return Err(format!("the mod stopped ({reason}), expected {want:?}"));
            }
            (Some(_), Some(_)) => return Ok(()),
            (Some(reason), None) => return Err(format!("the mod stopped: {reason}")),
            (None, Some(want)) => {
                return Err(format!(
                    "the mod kept running, expected it to stop ({want:?})"
                ));
            }
            (None, None) => {}
        }
        if owned == Some(false) {
            let line = self.command.as_deref().unwrap_or_default();
            let name = line.split(' ').next().unwrap_or_default();
            return Err(format!("the mod owns no command /{name}"));
        }
        if let Some(want) = &e.commands {
            same("commands", want, &game.commands())?;
        }
        if let Some(want) = &e.chat {
            let got: Vec<String> = out
                .iter()
                .filter_map(|o| match o {
                    Output::Chat { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect();
            same("chat", want, &got)?;
        }
        let feedback = |error: bool| {
            out.iter().rev().find_map(|o| match o {
                Output::Feedback { text, error: e } | Output::Reply { text, error: e, .. }
                    if *e == error =>
                {
                    Some(text.clone())
                }
                _ => None,
            })
        };
        if let Some(want) = &e.screen {
            let got = game.screen_lines().unwrap_or_default();
            same("screen", want, &got)?;
        }
        if let Some(want) = &e.net {
            let name = |id: &str| {
                let p = game.players.iter().find(|p| p.id == id);
                p.map_or(id.to_owned(), |p| p.name.clone())
            };
            let got: Vec<String> = out
                .iter()
                .filter_map(|o| match o {
                    Output::NetToClient {
                        player,
                        channel,
                        data,
                        ..
                    } => Some(format!(
                        "{}: {channel}={}",
                        name(player),
                        String::from_utf8_lossy(data)
                    )),
                    Output::NetToServer { channel, data, .. } => {
                        Some(format!("{channel}={}", String::from_utf8_lossy(data)))
                    }
                    _ => None,
                })
                .collect();
            same("net", want, &got)?;
        }
        if let Some(want) = &e.told {
            let name = |id: &str| {
                let p = game.players.iter().find(|p| p.id == id);
                p.map_or(id.to_owned(), |p| p.name.clone())
            };
            let got: Vec<String> = out
                .iter()
                .filter_map(|o| match o {
                    Output::Tell { player, text, .. } => Some(format!("{}: {text}", name(player))),
                    _ => None,
                })
                .collect();
            same("told", want, &got)?;
        }
        if let Some(want) = &e.broadcast {
            let got: Vec<String> = out
                .iter()
                .filter_map(|o| match o {
                    Output::Broadcast { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect();
            same("broadcast", want, &got)?;
        }
        if let Some(want) = &e.feedback {
            same("feedback", &Some(want.clone()), &feedback(false))?;
        }
        if let Some(want) = &e.error {
            same("error", &Some(want.clone()), &feedback(true))?;
        }
        if let Some(want) = &e.panel {
            let got: Vec<String> = game
                .panel()
                .map(|p| p.lines.iter().map(|l| l.text.clone()).collect())
                .unwrap_or_default();
            same("panel", want, &got)?;
        }
        if let Some(want) = &e.blocks {
            for b in want {
                let got = game.world.get(&(b.x, b.y, b.z));
                let got = got.map_or("minecraft:air", String::as_str);
                if got != b.state {
                    return Err(format!(
                        "block at {} {} {}: expected {}, got {got}",
                        b.x, b.y, b.z, b.state
                    ));
                }
            }
        }
        if let Some(want) = &e.corner {
            let got = game.panel().map(|p| corner_name(p.corner).to_string());
            same("corner", &Some(want.clone()), &got)?;
        }
        Ok(())
    }
}

fn corner_name(c: Corner) -> &'static str {
    match c {
        Corner::TopLeft => "top-left",
        Corner::TopRight => "top-right",
        Corner::BottomLeft => "bottom-left",
        Corner::BottomRight => "bottom-right",
    }
}

fn same<T: PartialEq + std::fmt::Debug>(what: &str, want: &T, got: &T) -> Result<(), String> {
    if want == got {
        return Ok(());
    }
    Err(format!(
        "{what}\n    expected: {want:?}\n    got:      {got:?}"
    ))
}
