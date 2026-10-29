//! Write Horizon mods in Rust (`docs/mods.md`).
//!
//! A mod is a type implementing [`Mod`], exported with [`export_mod!`] from
//! a `cdylib` crate built for `wasm32-unknown-unknown`;
//! `cargo horizon-mod build` turns it into the component the game loads.
//!
//! ```ignore
//! use horizon_mod_sdk::{Mod, Registration, chat};
//!
//! struct Hello;
//! impl Mod for Hello {
//!     fn init() -> Result<(Self, Registration), String> {
//!         Ok((Hello, Registration::new().command("hello")))
//!     }
//!     fn command(&mut self, _name: &str, _args: &str) -> Result<Option<String>, String> {
//!         chat::show("Hello from a mod!");
//!         Ok(None)
//!     }
//! }
//! horizon_mod_sdk::export_mod!(Hello);
//! ```
//!
//! Every interface but `log` is a capability: list the ones the mod uses in
//! `horizon-mod.toml` (`game-read`, `chat`, `hud`, `storage`, `command`, `ui`;
//! a server mod `storage` and `players`). Server mods: [`server`].
//!
//! Widgets (capability `ui`): a mod shows a screen it owns with
//! [`ui::open_screen`] and a page in Options > Mods with
//! [`ui::set_settings`]; what the player does with them comes back as
//! [`Mod::ui_select`], [`Mod::ui_closed`] and [`Mod::ui_settings_opened`].

wit_bindgen::generate!({
    path: "wit",
    world: "horizon-ui-mod",
    pub_export_macro: true,
    export_macro_name: "__export_horizon_mod",
    default_bindings_module: "horizon_mod_sdk",
});

pub use exports::horizon::mods::events::{Guest as __Guest, Registration};
pub use exports::horizon::mods::ui_events::Guest as __UiEventsGuest;
pub use horizon::mods::{
    browser, chat, command, game, hud, input, log, recipes, render, storage, ui,
};

/// The game's calls into a mod. Only the hooks [`Registration`] asks for
/// are called; the others may stay as their defaults.
pub trait Mod: Sized + 'static {
    /// Once, after loading: the mod's state and the hooks it wants. An error
    /// keeps the mod off and is shown to the player.
    fn init() -> Result<(Self, Registration), String>;
    /// Every game tick, when [`Registration::tick`] is set.
    fn tick(&mut self, _tick: u64) {}
    /// Every rendered frame, when [`Registration::frame`] is set.
    fn frame(&mut self, _seconds: f32) {}
    /// `/name args` for a command the mod registered. `Ok(Some(text))` is
    /// shown as feedback, `Err(text)` in red.
    fn command(&mut self, _name: &str, _args: &str) -> Result<Option<String>, String> {
        Err("This command does nothing".into())
    }
    /// A chat line the player received, when [`Registration::chat`] is set.
    fn chat(&mut self, _text: &str) {}
    /// A row of one of the mod's `list` widgets was picked (capability
    /// `ui`): the widget's `id` and the row.
    fn ui_select(&mut self, _id: u32, _row: u32) {}
    /// The player left the mod's screen (Esc or Done; capability `ui`).
    fn ui_closed(&mut self) {}
    /// The player opened the mod's settings page (capability `ui`): call
    /// [`ui::set_settings`] for fresh widgets.
    fn ui_settings_opened(&mut self) {}
}

impl Registration {
    /// No commands and no hooks.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            tick: false,
            frame: false,
            chat: false,
        }
    }
    /// Adds `/name`.
    pub fn command(mut self, name: &str) -> Self {
        self.commands.push(name.into());
        self
    }
    pub fn on_tick(mut self) -> Self {
        self.tick = true;
        self
    }
    pub fn on_frame(mut self) -> Self {
        self.frame = true;
        self
    }
    pub fn on_chat(mut self) -> Self {
        self.chat = true;
        self
    }
}
impl Default for Registration {
    fn default() -> Self {
        Self::new()
    }
}

/// `log::write` at info level.
pub fn info(message: &str) {
    log::write(log::Level::Info, message);
}
/// `log::write` at warn level.
pub fn warn(message: &str) {
    log::write(log::Level::Warn, message);
}

/// Exports a [`Mod`] as the component's `events` interface. The state lives
/// in a thread local: a mod instance is single-threaded.
#[macro_export]
macro_rules! export_mod {
    ($t:ty) => {
        const _: () = {
            use ::std::cell::RefCell;
            ::std::thread_local! {
                static STATE: RefCell<Option<$t>> = const { RefCell::new(None) };
            }
            fn with(f: impl FnOnce(&mut $t)) {
                STATE.with(|s| {
                    if let Some(m) = s.borrow_mut().as_mut() {
                        f(m)
                    }
                })
            }
            struct Export;
            impl $crate::__Guest for Export {
                fn init() -> Result<$crate::Registration, String> {
                    let (m, registration) = <$t as $crate::Mod>::init()?;
                    STATE.with(|s| *s.borrow_mut() = Some(m));
                    Ok(registration)
                }
                fn tick(tick: u64) {
                    with(|m| $crate::Mod::tick(m, tick))
                }
                fn frame(seconds: f32) {
                    with(|m| $crate::Mod::frame(m, seconds))
                }
                fn command(name: String, args: String) -> Result<Option<String>, String> {
                    let mut out = Err("The mod is not running".to_string());
                    with(|m| out = $crate::Mod::command(m, &name, &args));
                    out
                }
                fn chat(text: String) {
                    with(|m| $crate::Mod::chat(m, &text))
                }
            }
            impl $crate::__UiEventsGuest for Export {
                fn select(id: u32, row: u32) {
                    with(|m| $crate::Mod::ui_select(m, id, row))
                }
                fn closed() {
                    with(|m| $crate::Mod::ui_closed(m))
                }
                fn settings_opened() {
                    with(|m| $crate::Mod::ui_settings_opened(m))
                }
            }
            // Only WebAssembly has the component's exports: a host build (cargo test or
            // clippy on the workspace) checks the mod without them, which macOS
            // couldn't link (its exported-symbols list cuts names at the `#`).
            #[cfg(target_arch = "wasm32")]
            $crate::__export_horizon_mod!(Export);
            #[cfg(not(target_arch = "wasm32"))]
            let _ = (
                <Export as $crate::__Guest>::init,
                <Export as $crate::__UiEventsGuest>::select,
            );
        };
    };
}

/// Server mods (manifest `side = "server"`, `api = "0.3"`, experimental): a
/// type implementing [`ServerMod`](server::ServerMod), exported with
/// [`export_server_mod!`]. It runs where the world is simulated, and reads
/// and messages the players with capability `players`.
pub mod server {
    wit_bindgen::generate!({
        path: "wit",
        world: "horizon-block-mod",
        pub_export_macro: true,
        export_macro_name: "__export_horizon_server_mod",
        default_bindings_module: "horizon_mod_sdk::server",
        with: {
            "horizon:mods/log@0.1.0": crate::log,
            "horizon:mods/storage@0.1.0": crate::storage,
        },
    });

    pub use exports::horizon::mods::block_entity_events::{BlockEntity, Guest as __BlockGuest};
    pub use exports::horizon::mods::block_use_events::{Guest as __UseGuest, UsedBlock};
    pub use exports::horizon::mods::menu_events::Guest as __MenuGuest;
    pub use exports::horizon::mods::server_events::{
        Argument, ArgumentKind, ArgumentValue, BlockPos, Guest as __Guest, Registration,
        ServerCommand,
    };
    pub use horizon::mods::block_entities;
    pub use horizon::mods::displays::{self, Shape, Transform};
    pub use horizon::mods::energy::{self, Storage as EnergyStorage};
    pub use horizon::mods::items::{self, Stack};
    pub use horizon::mods::machine_recipes;
    pub use horizon::mods::menus::{self, MenuItem, MenuSlot, SlotMode};
    pub use horizon::mods::multiblocks;
    pub use horizon::mods::players::{self, GameMode, Player};
    pub use horizon::mods::world::{self, BlockChange};

    /// The server's calls into a mod. Only the hooks [`Registration`] asks
    /// for are called; the others may stay as their defaults.
    pub trait ServerMod: Sized + 'static {
        /// Once, after loading: the mod's state and the hooks it wants. An
        /// error keeps the mod off and is logged.
        fn init() -> Result<(Self, Registration), String>;
        /// Every server tick, when [`Registration::tick`] is set.
        fn tick(&mut self, _tick: u64) {}
        /// When [`Registration::joins`] is set.
        fn player_joined(&mut self, _player: &Player) {}
        fn player_left(&mut self, _player: &Player) {}
        /// A chat message a player sent, when [`Registration::chat`] is set.
        fn chat(&mut self, _player: &Player, _text: &str) {}
        /// `/name args` typed by `player`, parsed as the command's
        /// arguments say ([`Registration::command_with`]). `Ok(Some(text))`
        /// is their feedback, `Err(text)` their error in red.
        fn command(
            &mut self,
            _player: &Player,
            _name: &str,
            _args: &Args,
        ) -> Result<Option<String>, String> {
            Err("This command does nothing".into())
        }
        /// When [`Registration::blocks`] is set: `state` is what was there.
        fn block_broken(&mut self, _player: &Player, _pos: BlockPos, _state: &str) {}
        fn block_placed(&mut self, _player: &Player, _pos: BlockPos, _state: &str) {}
        /// Capability `block-entities`: the blocks (`minecraft:lodestone`)
        /// whose placed copies get this mod's block entities. Asked once,
        /// after [`ServerMod::init`].
        fn block_entity_blocks() -> Vec<String> {
            Vec::new()
        }
        /// Each server tick, after [`ServerMod::tick`]: the mod's block
        /// entities in loaded chunks. Change one with
        /// [`block_entities::set`].
        fn block_entity_tick(&mut self, _tick: u64, _entities: &[BlockEntity]) {}
        /// A block entity's block is gone: its last data, with what stands
        /// there now.
        fn block_entity_removed(&mut self, _entity: &BlockEntity) {}
        /// Capability `menus`: `player` clicked slot `slot` of the mod's
        /// open menu `menu` (`button` 0 left, 1 right).
        fn menu_clicked(
            &mut self,
            _player: &Player,
            _menu: &str,
            _slot: u32,
            _button: u8,
            _shift: bool,
        ) {
        }
        /// `player` closed the mod's menu `menu`.
        fn menu_closed(&mut self, _player: &Player, _menu: &str) {}
        /// Among [`ServerMod::block_entity_blocks`], those whose use (a
        /// right click) the mod takes, in place of vanilla's: asked once.
        fn block_uses() -> Vec<String> {
            Vec::new()
        }
        /// `player` used (right-clicked) one of the blocks
        /// [`ServerMod::block_uses`] names.
        fn block_used(&mut self, _player: &Player, _block: &UsedBlock) {}
    }

    impl Transform {
        /// Not moved, scaled or turned.
        pub fn identity() -> Self {
            Self {
                translation: (0.0, 0.0, 0.0),
                scale: (1.0, 1.0, 1.0),
                rotation: (0.0, 0.0, 0.0, 1.0),
            }
        }
        pub fn translated(mut self, x: f32, y: f32, z: f32) -> Self {
            self.translation = (x, y, z);
            self
        }
        pub fn scaled(mut self, x: f32, y: f32, z: f32) -> Self {
            self.scale = (x, y, z);
            self
        }
        /// Turned by `degrees` about the vertical axis.
        pub fn turned_y(mut self, degrees: f32) -> Self {
            let half = degrees.to_radians() / 2.0;
            self.rotation = (0.0, half.sin(), 0.0, half.cos());
            self
        }
    }

    impl MenuItem {
        /// `count` of `item` in slot `slot`, with no components.
        pub fn new(slot: u32, item: &str, count: u32) -> Self {
            Self {
                slot,
                item: item.into(),
                count,
                components: String::new(),
            }
        }
        /// Shows `name` as the item's name, not in italics, and `lore` below
        /// it, in grey.
        pub fn named(mut self, name: &str, lore: &[&str]) -> Self {
            let text = |t: &str| {
                let t = t.replace('\\', "\\\\").replace('"', "\\\"");
                format!("{{text:\"{t}\",italic:false}}")
            };
            let mut components = format!("custom_name={}", text(name));
            if !lore.is_empty() {
                let lines: Vec<String> = lore
                    .iter()
                    .map(|l| {
                        let t = l.replace('\\', "\\\\").replace('"', "\\\"");
                        format!("{{text:\"{t}\",italic:false,color:\"gray\"}}")
                    })
                    .collect();
                components.push_str(&format!(",lore=[{}]", lines.join(",")));
            }
            self.components = format!("[{components}]");
            self
        }
    }

    impl Registration {
        /// No commands and no hooks.
        pub fn new() -> Self {
            Self {
                commands: Vec::new(),
                tick: false,
                joins: false,
                chat: false,
                blocks: false,
            }
        }
        /// Adds `/name`, taking free text ([`Args::text`]): one optional
        /// `text` argument named `args`.
        pub fn command(self, name: &str) -> Self {
            self.command_with(name, [Argument::text("args").optional()])
        }
        /// Adds `/name` with typed arguments (none: nothing after the name):
        /// players' chat boxes complete and check them, and the server
        /// parses them into [`Args`].
        pub fn command_with(
            mut self,
            name: &str,
            arguments: impl IntoIterator<Item = Argument>,
        ) -> Self {
            self.commands.push(ServerCommand {
                name: name.into(),
                arguments: arguments.into_iter().collect(),
            });
            self
        }
        pub fn on_tick(mut self) -> Self {
            self.tick = true;
            self
        }
        pub fn on_joins(mut self) -> Self {
            self.joins = true;
            self
        }
        pub fn on_chat(mut self) -> Self {
            self.chat = true;
            self
        }
        pub fn on_blocks(mut self) -> Self {
            self.blocks = true;
            self
        }
    }
    impl Default for Registration {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Argument {
        fn of(name: &str, kind: ArgumentKind) -> Self {
            Self {
                name: name.into(),
                kind,
                optional: false,
            }
        }
        /// One word: letters, digits and `_ - . +`.
        pub fn word(name: &str) -> Self {
            Self::of(name, ArgumentKind::Word)
        }
        /// The rest of the line: only last.
        pub fn text(name: &str) -> Self {
            Self::of(name, ArgumentKind::Text)
        }
        /// A whole number, at least `min` and at most `max` when given.
        pub fn integer(name: &str, min: Option<i32>, max: Option<i32>) -> Self {
            Self::of(name, ArgumentKind::Integer((min, max)))
        }
        pub fn number(name: &str) -> Self {
            Self::of(name, ArgumentKind::Number)
        }
        pub fn boolean(name: &str) -> Self {
            Self::of(name, ArgumentKind::Boolean)
        }
        /// One online player: a name, `@s` or `@p`.
        pub fn player(name: &str) -> Self {
            Self::of(name, ArgumentKind::Player)
        }
        /// `x y z`, each one also `~` or `~n`.
        pub fn block_pos(name: &str) -> Self {
            Self::of(name, ArgumentKind::BlockPos)
        }
        /// May be left out; optional arguments come after the required ones.
        pub fn optional(mut self) -> Self {
            self.optional = true;
            self
        }
    }

    /// A command's arguments: the line as typed, and the values the server
    /// parsed from it, by argument name.
    #[derive(Clone, Debug, Default)]
    pub struct Args {
        text: String,
        values: Vec<(String, ArgumentValue)>,
    }
    impl Args {
        pub fn new(text: String, values: Vec<(String, ArgumentValue)>) -> Self {
            Self { text, values }
        }
        /// The line after the command's name, as typed.
        pub fn text(&self) -> &str {
            &self.text
        }
        /// The value of argument `name`; `None` when it was left out.
        pub fn get(&self, name: &str) -> Option<&ArgumentValue> {
            self.values.iter().find(|(n, _)| n == name).map(|(_, v)| v)
        }
        pub fn integer(&self, name: &str) -> Option<i32> {
            match self.get(name)? {
                ArgumentValue::Integer(n) => Some(*n),
                _ => None,
            }
        }
        pub fn number(&self, name: &str) -> Option<f64> {
            match self.get(name)? {
                ArgumentValue::Number(n) => Some(*n),
                _ => None,
            }
        }
        /// A `word` or `text` argument.
        pub fn string(&self, name: &str) -> Option<&str> {
            match self.get(name)? {
                ArgumentValue::Word(s) | ArgumentValue::Text(s) => Some(s),
                _ => None,
            }
        }
        pub fn boolean(&self, name: &str) -> Option<bool> {
            match self.get(name)? {
                ArgumentValue::Boolean(b) => Some(*b),
                _ => None,
            }
        }
        pub fn player(&self, name: &str) -> Option<&Player> {
            match self.get(name)? {
                ArgumentValue::Player(p) => Some(p),
                _ => None,
            }
        }
        pub fn block_pos(&self, name: &str) -> Option<BlockPos> {
            match self.get(name)? {
                ArgumentValue::BlockPos(p) => Some(*p),
                _ => None,
            }
        }
    }
}

/// Exports a [`ServerMod`](server::ServerMod) as the component's
/// `server-events` interface, as [`export_mod!`] does for a client mod.
#[macro_export]
macro_rules! export_server_mod {
    ($t:ty) => {
        const _: () = {
            use ::std::cell::RefCell;
            use $crate::server::{Args, ArgumentValue, BlockPos, Player, Registration, ServerMod};
            ::std::thread_local! {
                static STATE: RefCell<Option<$t>> = const { RefCell::new(None) };
            }
            fn with(f: impl FnOnce(&mut $t)) {
                STATE.with(|s| {
                    if let Some(m) = s.borrow_mut().as_mut() {
                        f(m)
                    }
                })
            }
            struct Export;
            impl $crate::server::__Guest for Export {
                fn init() -> Result<Registration, String> {
                    let (m, registration) = <$t as ServerMod>::init()?;
                    STATE.with(|s| *s.borrow_mut() = Some(m));
                    Ok(registration)
                }
                fn tick(tick: u64) {
                    with(|m| m.tick(tick))
                }
                fn player_joined(player: Player) {
                    with(|m| m.player_joined(&player))
                }
                fn player_left(player: Player) {
                    with(|m| m.player_left(&player))
                }
                fn chat(player: Player, text: String) {
                    with(|m| m.chat(&player, &text))
                }
                fn command(
                    player: Player,
                    name: String,
                    args: String,
                    values: Vec<(String, ArgumentValue)>,
                ) -> Result<Option<String>, String> {
                    let mut out = Err("The mod is not running".to_string());
                    let args = Args::new(args, values);
                    with(|m| out = m.command(&player, &name, &args));
                    out
                }
                fn block_broken(player: Player, pos: BlockPos, state: String) {
                    with(|m| m.block_broken(&player, pos, &state))
                }
                fn block_placed(player: Player, pos: BlockPos, state: String) {
                    with(|m| m.block_placed(&player, pos, &state))
                }
            }
            impl $crate::server::__UseGuest for Export {
                fn uses() -> Vec<String> {
                    <$t as ServerMod>::block_uses()
                }
                fn used(player: Player, block: $crate::server::UsedBlock) {
                    with(|m| m.block_used(&player, &block))
                }
            }
            impl $crate::server::__MenuGuest for Export {
                fn clicked(player: Player, menu: String, slot: u32, button: u8, shift: bool) {
                    with(|m| m.menu_clicked(&player, &menu, slot, button, shift))
                }
                fn closed(player: Player, menu: String) {
                    with(|m| m.menu_closed(&player, &menu))
                }
            }
            impl $crate::server::__BlockGuest for Export {
                fn blocks() -> Vec<String> {
                    <$t as ServerMod>::block_entity_blocks()
                }
                fn tick(tick: u64, entities: Vec<$crate::server::BlockEntity>) {
                    with(|m| m.block_entity_tick(tick, &entities))
                }
                fn removed(entity: $crate::server::BlockEntity) {
                    with(|m| m.block_entity_removed(&entity))
                }
            }
            #[cfg(target_arch = "wasm32")]
            $crate::server::__export_horizon_server_mod!(Export);
            #[cfg(not(target_arch = "wasm32"))]
            let _ = <Export as $crate::server::__Guest>::init;
        };
    };
}
