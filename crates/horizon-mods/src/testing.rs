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
    /// The mobs of the mods (capability `entities`), in the Overworld: what
    /// the mods spawned, steered and hurt. Every mob has 20 health.
    pub entities: Vec<crate::EntityInfo>,
    /// The mods' display entities (capability `displays`), as the game keeps
    /// them: shown or changed by `show` and `show-item`, moved by `move`,
    /// carried by `ride`. Never saved, as in the game.
    pub displays: Vec<TestDisplay>,
    /// Mobs that take a right click (spawned with `takes-use`).
    entity_takes_use: std::collections::BTreeSet<u32>,
    /// The next entity id a spawn takes.
    entity_next: u32,
    /// Mobs to tell the mods died, and are gone, on the next tick.
    entity_died: Vec<crate::EntityInfo>,
    entity_removed: Vec<crate::EntityInfo>,
    /// A step has lent the mobs for the calls of its parts.
    entity_lent: bool,
    /// The signals a script put on blocks of the Overworld (capability
    /// `redstone`): what the block there reads on each face, as the game
    /// would read it from the neighbours.
    pub redstone_in: std::collections::BTreeMap<(i32, i32, i32), crate::redstone::Sides>,
}

/// The vanilla mobs a test's mods may spawn.
const TEST_MOBS: [&str; 8] = [
    "minecraft:zombie",
    "minecraft:skeleton",
    "minecraft:creeper",
    "minecraft:iron_golem",
    "minecraft:cow",
    "minecraft:pig",
    "minecraft:sheep",
    "minecraft:villager",
];

/// One of the mods' display entities (capability `displays`), as a test
/// reads it after a step.
#[derive(Clone, Debug)]
pub struct TestDisplay {
    /// The mod that showed it.
    pub mod_id: String,
    pub name: String,
    /// `text`, `block` or `item`.
    pub kind: &'static str,
    /// What it shows: the text, the block's state, or `item x<count>`.
    pub what: String,
    pub pos: [f64; 3],
    /// The mod's mob that carries it (`ride`), if it rides one.
    pub riding: Option<u32>,
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

/// A fresh scratch folder for a [`TestGame`]: every load, single or pair,
/// takes its own, numbered from one counter so two games never share one.
fn scratch_dir() -> PathBuf {
    static RUN: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "horizon-mod-test-{}-{}",
        std::process::id(),
        RUN.fetch_add(1, Ordering::Relaxed)
    ))
}

impl TestGame {
    /// Starts the mod at `hmod`, as the game would with it turned on. Errs
    /// when it doesn't start, with the game's reason.
    pub fn load(hmod: &Path) -> Result<TestGame, String> {
        let dir = scratch_dir();
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
            entities: Vec::new(),
            displays: Vec::new(),
            entity_takes_use: Default::default(),
            entity_next: 1000,
            entity_died: Vec::new(),
            entity_removed: Vec::new(),
            entity_lent: false,
            redstone_in: Default::default(),
        };
        copy_mod(hmod, &copy)?;
        game.start()?;
        Ok(game)
    }

    fn start(&mut self) -> Result<(), String> {
        self.start_paths(std::slice::from_ref(&self.hmod.clone()))
    }

    /// Starts the mods at `paths` together in one sandbox, their library
    /// (the one others `requires`) first, as the game starts them. Errs
    /// when the first one doesn't start, with the game's reason.
    fn start_paths(&mut self, paths: &[PathBuf]) -> Result<(), String> {
        let recipes = machine_recipes_of(&self.hmod);
        let multiblocks = multiblocks_of(&self.hmod);
        self.mods = Mods::load_for(Runs::Both, paths, |_| true, |_| false, recipes, multiblocks);
        match &self.mods.entries()[0].status {
            Status::Failed(reason) => Err(format!("the mod didn't start: {reason}")),
            _ if !self.mods.is_running() => Err("the mod has no code to run".into()),
            _ => Ok(()),
        }
    }

    /// Starts two mods together, a library and the mod that needs it
    /// (capability `inter-mod`): the library first, whichever order they
    /// are given in, into one scratch copy the pair shares. Errs when
    /// either doesn't start. Their scripts run on it as on one game; a
    /// `reload` step reloads the first mod only, and one recipes table is
    /// built, from the first of the two that carries one.
    pub fn load_pair(a: &Path, b: &Path) -> Result<TestGame, String> {
        let dir = scratch_dir();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let copy_a = dir.join(a.file_name().unwrap_or("mod.hmod".as_ref()));
        let copy_b = dir.join(b.file_name().unwrap_or("mod.hmod".as_ref()));
        copy_mod(a, &copy_a)?;
        copy_mod(b, &copy_b)?;
        let mut game = TestGame {
            mods: Mods::default(),
            hmod: copy_a.clone(),
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
            entities: Vec::new(),
            displays: Vec::new(),
            entity_takes_use: Default::default(),
            entity_next: 1000,
            entity_died: Vec::new(),
            entity_removed: Vec::new(),
            entity_lent: false,
            redstone_in: Default::default(),
        };
        game.start_paths(&[copy_a, copy_b])?;
        let failed = game
            .mods
            .entries()
            .iter()
            .find(|e| matches!(e.status, Status::Failed(_)))
            .map(|e| match &e.status {
                Status::Failed(reason) => format!("the mod didn't start: {reason}"),
                _ => unreachable!("found only Failed above"),
            });
        match failed {
            Some(e) => Err(e),
            None => Ok(game),
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
            self.mods.config_tick(self.tick);
            self.entities_tick();
            self.items_tick();
            // Capability `inter-mod`: the mods' messages to each other, as
            // the game's tick delivers them, at its end.
            self.mods.inter_mod_tick();
        }
    }
    /// Lends the mobs to the mods for the calls until
    /// [`TestGame::settle_entities`]; `can_spawn` as the game has it.
    fn lend_entities(&mut self, can_spawn: bool) {
        self.mods.lend_entities(crate::EntityState {
            next_id: self.entity_next,
            dimension: "minecraft:overworld".into(),
            can_spawn,
            kinds: std::sync::Arc::new(TEST_MOBS.iter().map(|k| (*k).to_owned()).collect()),
            entities: self.entities.clone(),
        });
    }
    /// What the game does before it calls a redstone mod: reads the signals
    /// around each of its block entities (here, the ones a script set; zero
    /// elsewhere) into the store.
    fn feed_redstone(&mut self) {
        for id in self.mods.redstone_mods() {
            let keys: Vec<crate::block_entities::Key> = self
                .block_entities
                .of(&id)
                .map(|(k, _)| k.clone())
                .collect();
            for key in keys {
                let [x, y, z] = key.1;
                let read = match key.0.as_str() {
                    "minecraft:overworld" => self.redstone_in.get(&(x, y, z)).copied(),
                    _ => None,
                };
                self.block_entities
                    .redstone
                    .inputs
                    .insert(key, read.unwrap_or([0; 6]));
            }
        }
    }
    /// Takes the mobs back and applies what the mods did to them, as the game
    /// does after its calls.
    fn settle_entities(&mut self) {
        if let Some(state) = self.mods.take_entities() {
            self.entity_next = self.entity_next.max(state.next_id);
        }
        for o in self.mods.take_entity_outputs() {
            match o {
                Output::SpawnEntity {
                    mod_id,
                    id,
                    pos,
                    options,
                } => {
                    let kind = if options.kind.contains(':') {
                        options.kind.clone()
                    } else {
                        format!("minecraft:{}", options.kind)
                    };
                    if options.takes_use {
                        self.entity_takes_use.insert(id);
                    }
                    self.entities.push(crate::EntityInfo {
                        id,
                        owner: mod_id,
                        kind,
                        tag: options.tag,
                        dimension: "minecraft:overworld".into(),
                        pos,
                        yaw: 0.0,
                        health: options.health.map_or(20.0, |h| h.min(20.0)),
                        max_health: 20.0,
                        dying: false,
                    });
                }
                Output::TeleportEntity { mod_id, id, pos } => {
                    if let Some(e) = self.mob_of(&mod_id, id) {
                        e.pos = pos;
                    }
                }
                Output::PushEntity { .. } => {}
                Output::SetEntityHealth { mod_id, id, health } => {
                    if let Some(e) = self.mob_of(&mod_id, id) {
                        e.health = health.min(e.max_health);
                    }
                    self.reap_dying();
                }
                Output::RemoveEntity { mod_id, id } => {
                    if let Some(i) = self
                        .entities
                        .iter()
                        .position(|e| e.id == id && e.owner == mod_id)
                    {
                        let gone = self.entities.remove(i);
                        self.entity_removed.push(gone);
                    }
                }
                _ => {}
            }
        }
    }
    fn mob_of(&mut self, mod_id: &str, id: u32) -> Option<&mut crate::EntityInfo> {
        self.entities
            .iter_mut()
            .find(|e| e.id == id && e.owner == mod_id)
    }
    /// A mob with no health left starts dying: its mod is told on the next
    /// tick, and it is gone the tick after.
    fn reap_dying(&mut self) {
        for e in &mut self.entities {
            if e.health <= 0.0 && !e.dying {
                e.health = 0.0;
                e.dying = true;
                self.entity_died.push(e.clone());
            }
        }
    }
    /// The server tick's part for mobs: the deaths and removals since the
    /// last tick, then the tick, then what the mods did.
    fn entities_tick(&mut self) {
        if !self.mods.wants_entities() {
            return;
        }
        let was_lent = self.entity_lent;
        self.lend_entities(true);
        let told = std::mem::take(&mut self.entity_died);
        for e in &told {
            self.mods.entity_died(e);
        }
        // A mob that died vanishes the tick after its death was told.
        let (dead, alive): (Vec<_>, Vec<_>) = std::mem::take(&mut self.entities)
            .into_iter()
            .partition(|e| e.dying && !told.iter().any(|t| t.id == e.id));
        self.entities = alive;
        self.entity_removed.extend(dead);
        for e in std::mem::take(&mut self.entity_removed) {
            self.mods.entity_removed(&e);
        }
        let live = self.entities.clone();
        self.mods.entity_tick(self.tick, &live);
        self.settle_entities();
        if was_lent {
            self.lend_entities(true);
        }
    }
    /// `player` strikes the mob of the mods tagged `tag` for `damage`: its
    /// mod answers (capability `entities`), and what is left comes off its
    /// health. The damage dealt.
    pub fn strike_entity(
        &mut self,
        player: &ServerPlayer,
        tag: &str,
        damage: f32,
    ) -> Result<f32, String> {
        self.snapshot();
        let info = self.find_entity(tag)?;
        self.lend_entities(false);
        let dealt = self.mods.entity_hurt(player, &info, damage);
        self.settle_entities();
        if let Some(e) = self.entities.iter_mut().find(|e| e.id == info.id) {
            e.health = (e.health - dealt).max(0.0);
        }
        self.reap_dying();
        Ok(dealt)
    }
    /// `player` right-clicks the mob tagged `tag`: its mod hears it when the
    /// mob takes the use.
    pub fn interact_entity(&mut self, player: &ServerPlayer, tag: &str) -> Result<(), String> {
        self.snapshot();
        let info = self.find_entity(tag)?;
        if !self.entity_takes_use.contains(&info.id) {
            return Err(format!(
                "the mob tagged {tag} does not take the use (spawn it with takes_use)"
            ));
        }
        self.lend_entities(true);
        self.mods.entity_used(player, &info);
        self.settle_entities();
        Ok(())
    }
    fn find_entity(&self, tag: &str) -> Result<crate::EntityInfo, String> {
        self.entities
            .iter()
            .find(|e| e.tag == tag && !e.dying)
            .cloned()
            .ok_or_else(|| format!("no living mob of the mods is tagged {tag}"))
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
    /// The mod's config file (capability `config`), beside its copy.
    pub fn config_file(&self) -> PathBuf {
        let id = self.mods.entries()[0]
            .manifest
            .as_ref()
            .map_or_else(String::new, |m| m.id.clone());
        self.dir.join(crate::config::DIR).join(format!("{id}.toml"))
    }
    /// The player edits the config file to `text` and the game looks at it
    /// (it does that every five seconds; a script need not wait).
    pub fn edit_config(&mut self, text: &str) -> Result<(), String> {
        let path = self.config_file();
        std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))
            .and_then(|()| std::fs::write(&path, text))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        self.snapshot();
        self.mods.config_reload();
        Ok(())
    }
    /// Tells the mods an event of capability `game-events`, written as the
    /// scripts write it: `hurt Alex 4 16` (name, points lost, health after),
    /// `died Alex fell from a high place` (name, then the cause),
    /// `respawned Alex`, `dimension Alex minecraft:overworld
    /// minecraft:the_nether`, `mode Alex survival creative`, `killed Alex
    /// minecraft:zombie 7` (name, kind, entity id) and `weather rain`.
    pub fn game_event(&mut self, line: &str) -> Result<(), String> {
        self.snapshot();
        let mut words = line.split_whitespace();
        let kind = words.next().ok_or("an empty game event")?;
        let rest: Vec<&str> = words.collect();
        if kind == "weather" {
            let w = rest.first().ok_or("weather needs clear, rain or thunder")?;
            self.mods.weather_changed(w);
            return Ok(());
        }
        let name = rest
            .first()
            .ok_or_else(|| format!("{kind} needs a player"))?;
        let player = self
            .players
            .iter()
            .find(|p| p.name == *name)
            .cloned()
            .ok_or_else(|| format!("{name} is not on the server"))?;
        let arg = |i: usize| -> Result<&str, String> {
            rest.get(i)
                .copied()
                .ok_or_else(|| format!("{kind} needs more: {line:?}"))
        };
        let number = |i: usize| -> Result<f32, String> {
            arg(i)?.parse::<f32>().map_err(|e| format!("{line:?}: {e}"))
        };
        let mode = |i: usize| -> Result<GameMode, String> {
            Ok(match arg(i)? {
                "survival" => GameMode::Survival,
                "creative" => GameMode::Creative,
                "adventure" => GameMode::Adventure,
                "spectator" => GameMode::Spectator,
                other => return Err(format!("{other:?} is not a game mode")),
            })
        };
        match kind {
            "hurt" => self.mods.player_hurt(&player, number(1)?, number(2)?),
            "died" => self.mods.player_died(&player, &rest[1..].join(" ")),
            "respawned" => self.mods.player_respawned(&player),
            "dimension" => self.mods.dimension_changed(&player, arg(1)?, arg(2)?),
            "mode" => self.mods.game_mode_changed(&player, mode(1)?, mode(2)?),
            "killed" => {
                let id = arg(2)?
                    .parse::<u32>()
                    .map_err(|e| format!("{line:?}: {e}"))?;
                self.mods.mob_killed(&player, arg(1)?, id);
            }
            other => return Err(format!("{other:?} is not a game event")),
        }
        Ok(())
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
    /// What the mod showed since the last call; the mods' display entities
    /// are settled from it, as the game's tick does with them.
    pub fn drain(&mut self) -> Vec<Output> {
        let out = self.mods.drain();
        self.settle_displays(&out);
        out
    }

    /// Applies what the mods did to their display entities, as the game's
    /// tick does: shown or changed (`show`, `show-item`), hidden, moved (a
    /// riding one put down first) and made to ride one of the mod's own
    /// mobs (`ride`), or put down.
    fn settle_displays(&mut self, out: &[Output]) {
        for o in out {
            match o {
                Output::ShowDisplay {
                    mod_id,
                    name,
                    pos,
                    shape,
                    ..
                } => {
                    let (kind, what) = match shape {
                        crate::DisplayShape::Block(state) => ("block", state.clone()),
                        crate::DisplayShape::Text(text) => ("text", text.clone()),
                    };
                    self.displays
                        .retain(|d| !(d.mod_id == *mod_id && d.name == *name));
                    self.displays.push(TestDisplay {
                        mod_id: mod_id.clone(),
                        name: name.clone(),
                        kind,
                        what,
                        pos: *pos,
                        riding: None,
                    });
                }
                Output::ShowItemDisplay {
                    mod_id,
                    name,
                    pos,
                    stack,
                    ..
                } => {
                    self.displays
                        .retain(|d| !(d.mod_id == *mod_id && d.name == *name));
                    self.displays.push(TestDisplay {
                        mod_id: mod_id.clone(),
                        name: name.clone(),
                        kind: "item",
                        what: format!("{} x{}", stack.item, stack.count),
                        pos: *pos,
                        riding: None,
                    });
                }
                Output::HideDisplay { mod_id, name } => {
                    self.displays
                        .retain(|d| !(d.mod_id == *mod_id && d.name == *name));
                }
                Output::MoveDisplay {
                    mod_id, name, pos, ..
                } => {
                    if let Some(d) = self
                        .displays
                        .iter_mut()
                        .find(|d| d.mod_id == *mod_id && d.name == *name)
                    {
                        d.pos = *pos;
                        d.riding = None;
                    }
                }
                Output::RideDisplay {
                    mod_id,
                    name,
                    entity,
                } => {
                    let Some(d) = self
                        .displays
                        .iter_mut()
                        .find(|d| d.mod_id == *mod_id && d.name == *name)
                    else {
                        continue;
                    };
                    if d.riding == *entity {
                        continue;
                    }
                    if let Some(id) = entity {
                        if self
                            .entities
                            .iter()
                            .any(|e| e.id == *id && e.owner == *mod_id)
                        {
                            d.riding = Some(*id);
                        }
                    } else {
                        d.riding = None;
                    }
                }
                _ => {}
            }
        }
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

/// A signal on one face of a block of the Overworld (capability
/// `redstone`), as the block there reads it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalSet {
    x: i32,
    y: i32,
    z: i32,
    /// `down`, `up`, `north`, `south`, `west` or `east`.
    side: String,
    /// 0 to 15; 0 takes the signal away.
    power: u8,
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
pub struct StrikeSet {
    /// The mob struck, by the tag its mod gave it.
    tag: String,
    damage: f32,
}

/// A player right-clicks a mob of a mod (capability `entities`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractSet {
    tag: String,
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
    /// Server mods: the player edits the mod's config file (capability
    /// `config`) to this text, which the mod hears at once, before `chat`.
    config_file: Option<String>,
    /// Server mods: events of capability `game-events` the mods are told, in
    /// order, after `net` and before `command`: `"hurt Alex 4 16"`, `"died
    /// Alex fell from a high place"`, `"respawned Alex"`, `"dimension Alex
    /// minecraft:overworld minecraft:the_nether"`, `"mode Alex survival
    /// creative"`, `"killed Alex minecraft:zombie 7"`, `"weather rain"`.
    #[serde(default)]
    game_events: Vec<String>,
    /// Server mods: the player strikes or right-clicks a mob of a mod
    /// (capability `entities`), after `hurt`.
    strike: Option<StrikeSet>,
    interact: Option<InteractSet>,
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
    /// Signals the blocks read on a face (capability `redstone`), before the
    /// rest of the step and standing until another step changes them.
    #[serde(default)]
    signals: Vec<SignalSet>,
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
    /// Text the mod's config file holds after the step (capability
    /// `config`): each string must be in it.
    config_file: Option<Vec<String>>,
    /// The particles and sounds the mods sent during the step (capability
    /// `effects`), in order: `particle minecraft:flame x10 @ 0.5 65.0 0.5`
    /// (name, count, place), `sound minecraft:block.anvil.land 1 1.5 block @
    /// 0.5 64.0 0.5` (name, volume, pitch, category, place) and `sound-to
    /// Alex minecraft:entity.player.levelup 1 1 player`.
    effects: Option<Vec<String>>,
    /// What the mods' blocks give out after the step (capability
    /// `redstone`), as `x y z down up north south west east`, with ` strong`
    /// after it for a strong signal, ordered by place.
    redstone: Option<Vec<String>>,
    /// The mods' mobs after the step (capability `entities`), as
    /// `tag kind x y z hp=health`, ordered by id; a dying one ends `dying`.
    entities: Option<Vec<String>>,
    /// The mods' display entities after the step (capability `displays`),
    /// as `kind what @ x y z` (`item minecraft:nether_star x1 @ 0.5 64.6
    /// 0.5`, `text 8000 FE @ …`, `block minecraft:redstone_block @ …`),
    /// ordered by name; a riding one ends ` riding <mob id>`.
    displays: Option<Vec<String>>,
    /// The damage the mob's mod let through the step's `strike`.
    dealt: Option<f32>,
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
        for s in &self.signals {
            let face = ["down", "up", "north", "south", "west", "east"]
                .iter()
                .position(|n| *n == s.side)
                .ok_or_else(|| {
                    format!("{:?} is not a face (down up north south west east)", s.side)
                })?;
            if s.power > 15 {
                return Err(format!("a signal is 0 to 15, not {}", s.power));
            }
            game.redstone_in.entry((s.x, s.y, s.z)).or_default()[face] = s.power;
        }
        game.feed_redstone();
        let store = std::mem::take(&mut game.block_entities);
        game.mods.lend_block_entities(store);
        game.mods.lend_player_items(Box::new(ScriptItems {
            stacks: game.stacks.clone(),
            written: Vec::new(),
        }));
        game.lend_entities(true);
        game.entity_lent = true;
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
            || self.hurt.is_some()
            || self.strike.is_some()
            || self.interact.is_some();
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
        if let Some(text) = &self.config_file {
            game.edit_config(text)?;
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
        let mut dealt = None;
        if let (Some(s), Some(p)) = (&self.strike, &who) {
            dealt = Some(game.strike_entity(p, &s.tag, s.damage)?);
        }
        if let (Some(s), Some(p)) = (&self.interact, &who) {
            game.interact_entity(p, &s.tag)?;
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
        for line in &self.game_events {
            game.game_event(line)?;
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
        game.entity_lent = false;
        game.settle_entities();
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
        if let Some(want) = &e.config_file {
            let text = std::fs::read_to_string(game.config_file()).unwrap_or_default();
            for w in want {
                if !text.contains(w.as_str()) {
                    return Err(format!(
                        "config file
    expected it to hold: {w:?}
    got:
{text}"
                    ));
                }
            }
        }
        if let Some(want) = &e.effects {
            const CATEGORIES: [&str; 10] = [
                "master", "music", "record", "weather", "block", "hostile", "neutral", "player",
                "ambient", "voice",
            ];
            let category = |c: i32| CATEGORIES.get(c as usize).copied().unwrap_or("?");
            let got: Vec<String> = out
                .iter()
                .filter_map(|o| match o {
                    Output::Particle { particle: p, .. } => Some(format!(
                        "particle {} x{} @ {:.1} {:.1} {:.1}",
                        p.name, p.count, p.pos[0], p.pos[1], p.pos[2]
                    )),
                    Output::Sound {
                        name,
                        pos,
                        volume,
                        pitch,
                        category: c,
                        ..
                    } => Some(format!(
                        "sound {name} {volume} {pitch} {} @ {:.1} {:.1} {:.1}",
                        category(*c),
                        pos[0],
                        pos[1],
                        pos[2]
                    )),
                    Output::SoundTo {
                        player,
                        name,
                        volume,
                        pitch,
                        category: c,
                    } => {
                        let who = game.players.iter().find(|p| p.id == *player);
                        let who = who.map_or(player.clone(), |p| p.name.clone());
                        Some(format!(
                            "sound-to {who} {name} {volume} {pitch} {}",
                            category(*c)
                        ))
                    }
                    _ => None,
                })
                .collect();
            same("effects", want, &got)?;
        }
        if let Some(want) = &e.redstone {
            let got: Vec<String> = game
                .block_entities
                .redstone
                .outputs()
                .map(|((_, [x, y, z]), o)| {
                    let p = o.power;
                    format!(
                        "{x} {y} {z} {} {} {} {} {} {}{}",
                        p[0],
                        p[1],
                        p[2],
                        p[3],
                        p[4],
                        p[5],
                        if o.strong { " strong" } else { "" }
                    )
                })
                .collect();
            same("redstone", want, &got)?;
        }
        if let Some(want) = &e.entities {
            let mut mobs: Vec<&crate::EntityInfo> = game.entities.iter().collect();
            mobs.sort_by_key(|m| m.id);
            let got: Vec<String> = mobs
                .iter()
                .map(|m| {
                    format!(
                        "{} {} {:.1} {:.1} {:.1} hp={}{}",
                        m.tag,
                        m.kind,
                        m.pos[0],
                        m.pos[1],
                        m.pos[2],
                        m.health,
                        if m.dying { " dying" } else { "" }
                    )
                })
                .collect();
            same("entities", want, &got)?;
        }
        if let Some(want) = &e.displays {
            let mut shown: Vec<&TestDisplay> = game.displays.iter().collect();
            shown.sort_by(|a, b| a.name.cmp(&b.name));
            let got: Vec<String> = shown
                .iter()
                .map(|d| {
                    format!(
                        "{} {} @ {:.1} {:.1} {:.1}{}",
                        d.kind,
                        d.what,
                        d.pos[0],
                        d.pos[1],
                        d.pos[2],
                        d.riding.map_or(String::new(), |id| format!(" riding {id}"))
                    )
                })
                .collect();
            same("displays", want, &got)?;
        }
        if let Some(want) = e.dealt {
            same("dealt", &Some(want), &dealt)?;
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
