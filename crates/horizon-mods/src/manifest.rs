//! `horizon-mod.toml`, a mod's manifest, and the `.hmod` file around it.
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    io::Read,
    path::{Path, PathBuf},
};

/// The manifest's file name, beside `mod.wasm`.
pub const MANIFEST: &str = "horizon-mod.toml";
/// The component's file name in a `.hmod` or a mod folder.
pub const WASM: &str = "mod.wasm";
/// The API (WIT package `horizon:mods`) versions this build runs: a mod
/// built for any 0.1.x SDK.
pub const API: &str = "0.1";
/// The manifest `api` lines this build loads. 0.2 is 0.1 plus content:
/// `assets/` and `data/` in the `.hmod`, with blocks and items declared in
/// `data/<id>/horizon/` (docs/modpacks.md). Code still links API 0.1.
/// 0.3 is a server mod (experimental): the `horizon-server-mod` world.
pub const APIS: [&str; 3] = [API, "0.2", SERVER_API];
/// The line of server mods, experimental: `side = "server"`.
pub const SERVER_API: &str = "0.3";
/// Namespaces no mod may claim: vanilla's, the convention tags' (mods add
/// to those, docs/mods.md) and the game's own.
const RESERVED_NAMESPACES: [&str; 3] = ["minecraft", "c", "horizon"];
/// A mod's data (capability `storage`), in `horizon-mods/` beside the mods
/// (`find` never mistakes it for a mod: it has no manifest).
pub const STORAGE_DIR: &str = ".horizon-mod-data";
/// The biggest `mod.wasm` a mod may ship.
const MAX_WASM: u64 = 32 << 20;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// `[a-z0-9_]`, 1 to 64 characters: the mod's folder, log and settings key.
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    /// The API line the mod was built for: `0.1`, or `0.2` for content.
    pub api: String,
    #[serde(default)]
    pub side: Side,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    /// Namespaces besides `id` that the mod's `assets/` and `data/` use, for
    /// a content mod whose files sit under other names than its id (Towns
    /// and Towers ships `kaisyn` and `towns_and_towers`). No two content mods
    /// that are on may share one (`content::clash`).
    #[serde(default)]
    pub namespaces: Vec<String>,
    /// The experimental capabilities the mod opts into (`["ui"]`): each one
    /// it lists in `capabilities` must be named here too, since it may change
    /// in any release (docs/api/stability.md). One no longer experimental is
    /// accepted here, so the manifest keeps loading once it is promoted.
    #[serde(default)]
    pub experimental: Vec<Capability>,
    /// The APIs this mod provides to other server mods, capability
    /// `inter-mod`'s `name@version` entries (`["spin-lib@1.0"]`).
    #[serde(default)]
    pub provides: Vec<String>,
    /// The APIs this mod needs, as `provides` spells them (`["spin-lib@1"]`):
    /// the game starts their providers first and keeps it off, with the
    /// reason, when no turned-on mod provides one.
    #[serde(default)]
    pub requires: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// Runs in the player's game only (HUD, keys, chat commands).
    #[default]
    Client,
    /// Runs where the world is simulated (rules, blocks): `api = "0.3"`.
    Server,
    /// Both, and the server tells clients which version it expects.
    Both,
}

/// What a mod may do beyond writing its log. Each is a WIT interface the
/// component imports; the player sees them before turning the mod on.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    GameRead,
    Chat,
    Hud,
    Storage,
    /// Runs vanilla commands as the player, with the player's permissions.
    Command,
    /// New blocks and items from the mod's `data/` (API 0.2, and API 0.3
    /// for a server mod): no WIT interface, the game reads them before the
    /// world loads.
    Registries,
    /// A screen the mod owns and a settings page in Options > Mods, drawn
    /// with the menus' own widgets (interface `ui`, calls `ui-events`).
    Ui,
    /// Shapes on the HUD and the camera's shake, drawn by the game's own
    /// renderer (interface `render`).
    Render,
    /// The use button on items the mod claims, and its own keys
    /// (interface `input`).
    Input,
    /// The players on the server and messages to them (server mods).
    Players,
    /// Blocks in the server's loaded chunks, and changes to them (server
    /// mods; client mods later, where the game runs the world).
    World,
    /// The recipes and items the game knows, for recipe viewers
    /// (interface `recipes`).
    Recipes,
    /// Data kept on placed copies of the mod's blocks, saved with the world
    /// (server mods).
    BlockEntities,
    /// Energy buffers on block entities, shared by every mod (server mods).
    Energy,
    /// Chest menus the mod opens for players, with its own items (server
    /// mods).
    Menus,
    /// Block models and text the mod shows in the world (server mods).
    Displays,
    /// Inventories on block entities, shared by every mod (server mods).
    Items,
    /// The recipes of the mod's own machines, from its `data/<id>/recipe/`
    /// files (server mods).
    MachineRecipes,
    /// The mod's own multiblock structures, from its
    /// `data/<id>/multiblock/` files, checked against the world (server
    /// mods).
    Multiblocks,
    /// Data the mod keeps on the world itself, saved with it (server mods).
    SavedData,
    /// Behaviour on the items the mod claims: their use and their blow,
    /// each tick held or worn, and bytes of its own on each stack (server
    /// mods).
    ItemHooks,
    /// Messages between the two halves of one mod, the server's and the
    /// player's game (server mods and client mods).
    Network,
    /// Mobs of the mod's own: vanilla mobs it marks, steers, hurts and
    /// removes (server mods).
    Entities,
    /// Fluid tanks on block entities, shared by every mod (server mods).
    Fluids,
    /// Redstone for the mod's own blocks: read the signals around them, give
    /// signals out (server mods).
    Redstone,
    /// Particles and sounds in the world, for the players near (server mods).
    Effects,
    /// Notices of what happens to players and the world (server mods).
    GameEvents,
    /// Settings the player edits in a file (server mods).
    Config,
    /// Messages to other server mods, and theirs: a library mod's API
    /// shared by the mods that need it (server mods).
    InterMod,
}
impl Capability {
    pub const ALL: [Capability; 29] = [
        Capability::GameRead,
        Capability::Chat,
        Capability::Hud,
        Capability::Storage,
        Capability::Command,
        Capability::Registries,
        Capability::Ui,
        Capability::Render,
        Capability::Input,
        Capability::Players,
        Capability::World,
        Capability::Recipes,
        Capability::BlockEntities,
        Capability::Energy,
        Capability::Menus,
        Capability::Displays,
        Capability::Items,
        Capability::MachineRecipes,
        Capability::Multiblocks,
        Capability::SavedData,
        Capability::ItemHooks,
        Capability::Network,
        Capability::Entities,
        Capability::Fluids,
        Capability::Redstone,
        Capability::Effects,
        Capability::GameEvents,
        Capability::Config,
        Capability::InterMod,
    ];
    /// Whether a mod of either side may have it, as a server mod's `storage`
    /// is also a client mod's, and either side's may carry content.
    pub fn both_sides(self) -> bool {
        matches!(
            self,
            Capability::Storage | Capability::Network | Capability::Registries
        )
    }
    /// Whether a server mod may have it: the others act on a local player.
    pub fn on_server(self) -> bool {
        matches!(
            self,
            Capability::Storage
                | Capability::Players
                | Capability::World
                | Capability::BlockEntities
                | Capability::Energy
                | Capability::Menus
                | Capability::Displays
                | Capability::Items
                | Capability::MachineRecipes
                | Capability::Multiblocks
                | Capability::SavedData
                | Capability::ItemHooks
                | Capability::Network
                | Capability::Entities
                | Capability::Fluids
                | Capability::Redstone
                | Capability::Effects
                | Capability::GameEvents
                | Capability::Config
                | Capability::InterMod
        )
    }
    /// The WIT interface it grants, in `horizon:mods`.
    pub fn interface(self) -> &'static str {
        match self {
            Capability::GameRead => "game",
            Capability::Chat => "chat",
            Capability::Hud => "hud",
            Capability::Storage => "storage",
            Capability::Command => "command",
            Capability::Registries => "registries",
            Capability::Ui => "ui",
            Capability::Render => "render",
            Capability::Input => "input",
            Capability::Players => "players",
            Capability::World => "world",
            Capability::Recipes => "recipes",
            Capability::BlockEntities => "block-entities",
            Capability::Energy => "energy",
            Capability::Menus => "menus",
            Capability::Displays => "displays",
            Capability::Items => "items",
            Capability::MachineRecipes => "machine-recipes",
            Capability::Multiblocks => "multiblocks",
            Capability::SavedData => "saved-data",
            Capability::ItemHooks => "item-hooks",
            Capability::Network => "network",
            Capability::Entities => "entities",
            Capability::Fluids => "fluids",
            Capability::Redstone => "redstone",
            Capability::Effects => "effects",
            Capability::GameEvents => "game-events",
            Capability::Config => "config",
            Capability::InterMod => "inter-mod",
        }
    }
    /// May change in any release: a mod uses it only with
    /// `experimental = ["<name>"]` in its manifest.
    pub fn is_experimental(self) -> bool {
        matches!(
            self,
            Capability::Ui
                | Capability::Render
                | Capability::Input
                | Capability::Recipes
                | Capability::BlockEntities
                | Capability::Energy
                | Capability::Menus
                | Capability::Displays
                | Capability::Items
                | Capability::MachineRecipes
                | Capability::Multiblocks
                | Capability::SavedData
                | Capability::ItemHooks
                | Capability::Network
                | Capability::Entities
                | Capability::Fluids
                | Capability::Redstone
                | Capability::Effects
                | Capability::GameEvents
                | Capability::Config
                | Capability::InterMod
        )
    }
    /// What the player reads before turning the mod on: "May <this>".
    pub fn describe(self) -> &'static str {
        match self {
            Capability::GameRead => "see your position, health and the time",
            Capability::Chat => "show you chat messages",
            Capability::Hud => "draw text on your screen",
            Capability::Storage => "save its own settings to disk",
            Capability::Command => "run commands as you, with your permissions",
            Capability::Registries => "add blocks and items to your worlds",
            Capability::Ui => "show its own screens and a settings page",
            Capability::Render => "draw on your screen, shake and zoom the camera",
            Capability::Input => {
                "see the keys you hold and your clicks, take over the use button on its own items and the mouse wheel, and add keys"
            }
            Capability::Players => "see who is on the server and message them",
            Capability::World => "read and change the world's blocks",
            Capability::Recipes => {
                "read the game's recipes and items and which recipes you know, place recipes in your crafting grid and, in creative mode, give you items"
            }
            Capability::BlockEntities => {
                "keep its own data on the blocks it claims, saved with the world"
            }
            Capability::Energy => {
                "store energy in its blocks and move energy between any mod's blocks"
            }
            Capability::Menus => "open menus of its own for you, and see what you click in them",
            Capability::Displays => {
                "show block models, items and text of its own in the world, moved smoothly or riding its mobs"
            }
            Capability::Items => "keep items in its blocks and move items between any mod's blocks",
            Capability::MachineRecipes => "read the recipes of its own machines from data files",
            Capability::Multiblocks => {
                "keep multiblock structures of its own: read the world's blocks to check them, and hear when they come together and break"
            }
            Capability::SavedData => "keep data of its own on the world, saved with the world",
            Capability::ItemHooks => {
                "act on the items it claims: their use and their blow, each tick held or worn, and data of its own on each stack"
            }
            Capability::Network => {
                "send messages to the other half of the mod, on the server or on your game, and hear its messages"
            }
            Capability::Config => {
                "keep settings in a file you can edit while the game runs, and read them"
            }
            Capability::GameEvents => {
                "be told when players are hurt, die, respawn or change dimension or game mode, when they kill mobs, and when the weather turns"
            }
            Capability::Effects => {
                "show particles and play sounds in the world for the players near them"
            }
            Capability::Redstone => {
                "read the redstone signals around its blocks and give redstone signals out of them"
            }
            Capability::Fluids => {
                "store fluids in its blocks and move fluids between any mod's blocks"
            }
            Capability::Entities => {
                "spawn mobs of its own that it moves, hurts and removes, and hear when they are struck, used and die"
            }
            Capability::InterMod => "send messages to other server mods, and hear theirs",
        }
    }
    /// The capability a component import needs, `Ok(None)` for one every
    /// mod has; `Err` for an import Horizon does not offer.
    pub fn of_import(name: &str) -> Result<Option<Capability>, String> {
        let unknown = || format!("imports {name}, which Horizon does not offer");
        let rest = name.strip_prefix("horizon:mods/").ok_or_else(unknown)?;
        let (interface, version) = rest.split_once('@').ok_or_else(unknown)?;
        if !version.starts_with("0.1.") {
            return Err(format!("needs API {version}; this Horizon runs {API}"));
        }
        if interface == "log" {
            return Ok(None);
        }
        // A recipe viewer's panel is part of capability `ui`.
        if interface == "browser" {
            return Ok(Some(Capability::Ui));
        }
        Capability::ALL
            .into_iter()
            .filter(|c| *c != Capability::Registries)
            .find(|c| c.interface() == interface)
            .map(Some)
            .ok_or_else(unknown)
    }
}
impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Capability::GameRead => "game-read",
            Capability::Chat => "chat",
            Capability::Hud => "hud",
            Capability::Storage => "storage",
            Capability::Command => "command",
            Capability::Registries => "registries",
            Capability::Ui => "ui",
            Capability::Render => "render",
            Capability::Input => "input",
            Capability::Players => "players",
            Capability::World => "world",
            Capability::Recipes => "recipes",
            Capability::BlockEntities => "block-entities",
            Capability::Energy => "energy",
            Capability::Menus => "menus",
            Capability::Displays => "displays",
            Capability::Items => "items",
            Capability::MachineRecipes => "machine-recipes",
            Capability::Multiblocks => "multiblocks",
            Capability::SavedData => "saved-data",
            Capability::ItemHooks => "item-hooks",
            Capability::Network => "network",
            Capability::Entities => "entities",
            Capability::Fluids => "fluids",
            Capability::Redstone => "redstone",
            Capability::Effects => "effects",
            Capability::GameEvents => "game-events",
            Capability::Config => "config",
            Capability::InterMod => "inter-mod",
        };
        f.write_str(name)
    }
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let m: Manifest = toml::from_str(text).map_err(|e| format!("{MANIFEST}: {e}"))?;
        let id_ok = (1..=64).contains(&m.id.len())
            && m.id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !id_ok {
            return Err(format!(
                "{MANIFEST}: id {:?} is not [a-z0-9_]{{1,64}}",
                m.id
            ));
        }
        if !APIS.contains(&m.api.as_str()) {
            return Err(format!(
                "built for API {}; this Horizon runs {}",
                m.api,
                APIS.join(" and ")
            ));
        }
        if m.capabilities.contains(&Capability::Registries)
            && !matches!(m.api.as_str(), "0.2" | SERVER_API)
        {
            return Err(format!(
                "{MANIFEST}: capability registries needs api = \"0.2\" or \"{SERVER_API}\" \
                 (0.3: a server mod may carry it too)"
            ));
        }
        let server = m.side == Side::Server;
        if server != (m.api == SERVER_API) {
            return Err(format!(
                "{MANIFEST}: a server mod is side = \"server\" with api = \"{SERVER_API}\", and only it"
            ));
        }
        if let Some(c) = m
            .capabilities
            .iter()
            .find(|c| c.on_server() != server && !c.both_sides())
        {
            let side = if server { "client" } else { "server" };
            return Err(format!("{MANIFEST}: capability {c} is for {side} mods"));
        }
        // A multiblock structure is anchored on one of the mod's block
        // entities and checked against the world's blocks: both must be
        // asked for beside it (the world read is the player's to see).
        if m.capabilities.contains(&Capability::Multiblocks)
            && [Capability::BlockEntities, Capability::World]
                .into_iter()
                .any(|c| !m.capabilities.contains(&c))
        {
            return Err(format!(
                "{MANIFEST}: capability multiblocks needs capabilities block-entities and world"
            ));
        }
        if let Some(c) = m
            .capabilities
            .iter()
            .find(|c| c.is_experimental() && !m.experimental.contains(c))
        {
            return Err(format!(
                "{MANIFEST}: capability {c} is experimental and may change in any release: \
                 add experimental = [\"{c}\"] to use it"
            ));
        }
        if !m.namespaces.is_empty() && !m.capabilities.contains(&Capability::Registries) {
            return Err(format!(
                "{MANIFEST}: namespaces are for content, which needs capability registries"
            ));
        }
        for (i, ns) in m.namespaces.iter().enumerate() {
            // Vanilla's namespace characters, at most as long as an id.
            let chars_ok = (1..=64).contains(&ns.len())
                && ns
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b));
            if !chars_ok {
                return Err(format!(
                    "{MANIFEST}: namespace {ns:?} is not [a-z0-9_.-]{{1,64}}"
                ));
            }
            if RESERVED_NAMESPACES.contains(&ns.as_str()) {
                return Err(format!(
                    "{MANIFEST}: namespace {ns} is not a mod's to claim (only to add to its tags)"
                ));
            }
            if *ns == m.id || m.namespaces[..i].contains(ns) {
                return Err(format!("{MANIFEST}: namespace {ns} is listed twice"));
            }
        }
        // A library mod's `provides` and `requires` (capability `inter-mod`):
        // `name@version` entries, the name a channel's characters, the
        // version major[.minor[.patch]] — both lists need the capability.
        for (list, what) in [(&m.provides, "provides"), (&m.requires, "requires")] {
            for (i, entry) in list.iter().enumerate() {
                let check = |ok: bool, text: String| ok.then_some(()).ok_or(text);
                let (name, version) = match api_of(entry) {
                    Some(pair) => pair,
                    None => {
                        return Err(format!("{MANIFEST}: {what} {entry:?} is not name@version"));
                    }
                };
                check(
                    (1..=64).contains(&name.len())
                        && name.bytes().all(|b| {
                            b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b)
                        }),
                    format!("{MANIFEST}: {what} name {name:?} is not [a-z0-9_.-]{{1,64}}"),
                )?;
                check(
                    version_ok(version),
                    format!("{MANIFEST}: {what} version {version:?} is not major[.minor[.patch]]"),
                )?;
                if list[..i].contains(entry) {
                    return Err(format!("{MANIFEST}: {what} {entry} is listed twice"));
                }
            }
            if !list.is_empty() && !m.capabilities.contains(&Capability::InterMod) {
                return Err(format!(
                    "{MANIFEST}: {what} are for library mods, which need capability inter-mod"
                ));
            }
        }
        Ok(m)
    }
    /// The namespaces the mod's content owns: its id, then `namespaces`.
    pub fn owned_namespaces(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.id.as_str()).chain(self.namespaces.iter().map(String::as_str))
    }
    /// Checks that the component's imports need only declared capabilities.
    pub fn check_imports<'a>(
        &self,
        imports: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), String> {
        for name in imports {
            if let Some(c) = Capability::of_import(name)?
                && !self.capabilities.contains(&c)
            {
                return Err(format!("uses capability {c}, missing from {MANIFEST}"));
            }
        }
        Ok(())
    }
}

/// One `provides`/`requires` entry as the manifest spells it, `name@version`,
/// split. None for a malformed one (a parsed manifest holds none).
pub fn api_of(entry: &str) -> Option<(&str, &str)> {
    let (name, version) = entry.split_once('@')?;
    (!name.is_empty() && !version.is_empty()).then_some((name, version))
}

/// A `provides`/`requires` version: `major[.minor[.patch]]`, digits.
fn version_ok(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    (1..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|p| (1..=9).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
}

/// A version as comparable numbers, missing parts 0 (`1` is `1.0.0`).
fn numbers(version: &str) -> (u32, u32, u32) {
    let mut parts = version.split('.');
    let part = |parts: &mut std::str::Split<'_, char>| {
        parts.next().and_then(|p| p.parse().ok()).unwrap_or(0)
    };
    (part(&mut parts), part(&mut parts), part(&mut parts))
}

/// Whether `provides` (one manifest's entry) serves `requires` (another's
/// need): the same name, the same major version and one at least as new —
/// a need of `1` takes any `1.x`, a need of `1.2` takes `1.2` and later in
/// the 1 line, and `2` never serves `1`.
pub fn serves(provides: &str, requires: &str) -> bool {
    match (api_of(provides), api_of(requires)) {
        (Some((pn, pv)), Some((rn, rv))) => {
            pn == rn && numbers(pv).0 == numbers(rv).0 && numbers(pv) >= numbers(rv)
        }
        _ => false,
    }
}

/// A mod as found on disk: its manifest and component bytes.
pub struct Package {
    pub manifest: Manifest,
    /// None for a content-only mod (capability `registries`, no code).
    pub wasm: Option<Vec<u8>>,
}
impl Package {
    /// A `.hmod` (a zip holding `horizon-mod.toml`, `mod.wasm` and, for
    /// content, `assets/` and `data/`) or a folder holding the same.
    pub fn read(path: &Path) -> Result<Self, String> {
        let (toml, wasm) = if path.is_dir() {
            let toml = std::fs::read_to_string(path.join(MANIFEST));
            let wasm = path.join(WASM);
            let wasm = match wasm.exists() {
                true => Some(std::fs::read(wasm).map_err(|e| format!("{WASM}: {e}"))?),
                false => None,
            };
            (toml.map_err(|e| format!("{MANIFEST}: {e}"))?, wasm)
        } else {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            let mut entry = |name: &str| -> Result<Option<Vec<u8>>, String> {
                let f = match zip.by_name(name) {
                    Ok(f) => f,
                    Err(zip::result::ZipError::FileNotFound) => return Ok(None),
                    Err(e) => return Err(format!("{name}: {e}")),
                };
                let mut out = Vec::new();
                f.take(MAX_WASM + 1)
                    .read_to_end(&mut out)
                    .map_err(|e| e.to_string())?;
                Ok(Some(out))
            };
            let toml = entry(MANIFEST)?.ok_or_else(|| format!("{MANIFEST}: missing"))?;
            let toml = String::from_utf8(toml).map_err(|_| format!("{MANIFEST}: not UTF-8"))?;
            (toml, entry(WASM)?)
        };
        if wasm.as_ref().is_some_and(|w| w.len() as u64 > MAX_WASM) {
            return Err(format!("{WASM} is over {} MiB", MAX_WASM >> 20));
        }
        let manifest = Manifest::parse(&toml)?;
        if wasm.is_none() && !manifest.capabilities.contains(&Capability::Registries) {
            return Err(format!(
                "{WASM}: missing (only a content mod, with capability registries, may have no code)"
            ));
        }
        Ok(Package { manifest, wasm })
    }
    /// The `.hmod` bytes for `manifest_text` and a component.
    pub fn write(manifest_text: &str, wasm: &[u8]) -> Result<Vec<u8>, String> {
        Self::write_files(manifest_text, Some(wasm), &[])
    }
    /// The `.hmod` bytes for `manifest_text`, a component if the mod has
    /// code, and content `files` (`assets/...`, `data/...`) by path.
    pub fn write_files(
        manifest_text: &str,
        wasm: Option<&[u8]>,
        files: &[(&str, &[u8])],
    ) -> Result<Vec<u8>, String> {
        use std::io::Write;
        Manifest::parse(manifest_text)?;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        let err = |e: zip::result::ZipError| e.to_string();
        let mut put = |name: &str, bytes: &[u8]| -> Result<(), String> {
            zip.start_file(name, options).map_err(err)?;
            zip.write_all(bytes).map_err(|e| e.to_string())
        };
        put(MANIFEST, manifest_text.as_bytes())?;
        if let Some(wasm) = wasm {
            put(WASM, wasm)?;
        }
        for (name, bytes) in files {
            crate::content::check_path(name)?;
            put(name, bytes)?;
        }
        Ok(zip.finish().map_err(err)?.into_inner())
    }
}

/// The `.hmod` files and mod folders in `dir`, by name.
pub fn find(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "hmod") || p.join(MANIFEST).is_file())
        .collect();
    found.sort();
    found
}

/// Turns a `wasm32-unknown-unknown` cdylib built with the SDK into the
/// component the game loads.
#[cfg(feature = "pack")]
pub fn componentize(core: &[u8]) -> Result<Vec<u8>, String> {
    wit_component::ComponentEncoder::default()
        .module(core)
        .and_then(|e| e.validate(true).encode())
        .map_err(|e| format!("{e:#}"))
}

/// The names a component imports: `horizon:mods/game@0.1.0`, ...
#[cfg(feature = "pack")]
pub fn component_imports(component: &[u8]) -> Result<Vec<String>, String> {
    use wit_parser::{WorldItem, WorldKey};
    let wit_component::DecodedWasm::Component(resolve, world) =
        wit_component::decode(component).map_err(|e| format!("{e:#}"))?
    else {
        return Err("a WIT package, not a component".into());
    };
    let names = resolve.worlds[world]
        .imports
        .iter()
        .map(|(key, item)| match (key, item) {
            (WorldKey::Interface(id), _) | (_, WorldItem::Interface { id, .. }) => resolve
                .id_of(*id)
                .unwrap_or_else(|| resolve.name_world_key(key)),
            _ => resolve.name_world_key(key),
        });
    Ok(names.collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
id = "coords_hud"
name = "Coordinates HUD"
version = "0.1.0"
api = "0.1"
capabilities = ["game-read", "hud"]
"#;

    #[test]
    fn parses_and_checks_imports() {
        let m = Manifest::parse(EXAMPLE).unwrap();
        assert_eq!(m.side, Side::Client);
        assert_eq!(m.capabilities, [Capability::GameRead, Capability::Hud]);
        let ok = [
            "horizon:mods/log@0.1.0",
            "horizon:mods/game@0.1.0",
            "horizon:mods/hud@0.1.2",
        ];
        assert_eq!(m.check_imports(ok), Ok(()));
        let chat = m.check_imports(["horizon:mods/chat@0.1.0"]);
        assert_eq!(
            chat,
            Err("uses capability chat, missing from horizon-mod.toml".into())
        );
        assert!(m.check_imports(["wasi:filesystem/types@0.2.0"]).is_err());
        assert!(m.check_imports(["horizon:mods/game@0.2.0"]).is_err());
        let storage = m.check_imports(["horizon:mods/storage@0.1.0"]);
        assert_eq!(
            storage,
            Err("uses capability storage, missing from horizon-mod.toml".into())
        );
        assert_eq!(
            m.check_imports(["horizon:mods/command@0.1.0"]),
            Err("uses capability command, missing from horizon-mod.toml".into())
        );
        let with_storage = EXAMPLE.replace("\"hud\"", "\"hud\", \"storage\"");
        let m = Manifest::parse(&with_storage).unwrap();
        assert_eq!(m.check_imports(["horizon:mods/storage@0.1.0"]), Ok(()));
    }

    #[test]
    fn refuses_bad_ids_and_apis() {
        let bad_id = EXAMPLE.replace("coords_hud", "Coords HUD");
        assert!(Manifest::parse(&bad_id).unwrap_err().contains("is not"));
        let bad_api = EXAMPLE.replace("api = \"0.1\"", "api = \"0.4\"");
        assert!(Manifest::parse(&bad_api).unwrap_err().contains("API 0.4"));
        let unknown = format!("{EXAMPLE}network = true\n");
        assert!(Manifest::parse(&unknown).is_err());
    }

    #[test]
    fn server_mods_are_api_0_3_with_server_capabilities() {
        let server = |api: &str, side: &str, caps: &str| {
            Manifest::parse(&format!(
                "id = \"w\"\nname = \"W\"\nversion = \"1\"\napi = \"{api}\"\nside = \"{side}\"\ncapabilities = [{caps}]\n"
            ))
        };
        let m = server("0.3", "server", "\"players\", \"storage\"").unwrap();
        let imports = ["horizon:mods/players@0.1.0", "horizon:mods/storage@0.1.0"];
        assert_eq!(m.check_imports(imports), Ok(()));
        for (api, side) in [("0.3", "client"), ("0.3", "both"), ("0.1", "server")] {
            let e = server(api, side, "").unwrap_err();
            assert!(e.contains("a server mod is side"), "{e}");
        }
        let e = server("0.3", "server", "\"hud\"").unwrap_err();
        assert!(e.contains("capability hud is for client mods"), "{e}");
        let e = server("0.1", "client", "\"players\"").unwrap_err();
        assert!(e.contains("capability players is for server mods"), "{e}");
    }

    #[test]
    fn a_server_mod_may_carry_registries() {
        let server = |caps: &str, extra: &str| {
            Manifest::parse(&format!(
                "id = \"m\"\nname = \"M\"\nversion = \"1\"\napi = \"0.3\"\nside = \"server\"\n\
                 capabilities = [{caps}]\n{extra}"
            ))
        };
        // Content beside the code, and namespaces as a content mod lists
        // them, under the same rules.
        let m = server("\"players\", \"storage\", \"registries\"", "").unwrap();
        assert!(m.capabilities.contains(&Capability::Registries));
        let m = server("\"players\", \"registries\"", "namespaces = [\"kaisyn\"]\n").unwrap();
        assert_eq!(m.owned_namespaces().collect::<Vec<_>>(), ["m", "kaisyn"]);
        // API 0.1 still refuses it, naming the lines that may carry it.
        let e = Manifest::parse(
            "id = \"m\"\nname = \"M\"\nversion = \"1\"\napi = \"0.1\"\ncapabilities = [\"registries\"]\n",
        )
        .unwrap_err();
        assert!(
            e.contains("capability registries needs api = \"0.2\" or \"0.3\""),
            "{e}"
        );
        // A content mod of either side still parses: the capability belongs
        // to both sides.
        for side in ["client", "both"] {
            let m = Manifest::parse(&format!(
                "id = \"m\"\nname = \"M\"\nversion = \"1\"\napi = \"0.2\"\nside = \"{side}\"\n\
                 capabilities = [\"registries\"]\n"
            ))
            .unwrap();
            assert!(m.capabilities.contains(&Capability::Registries), "{side}");
        }
    }

    #[test]
    fn checks_extra_namespaces() {
        let content = "id = \"t_and_t\"\nname = \"T\"\nversion = \"1\"\napi = \"0.2\"\ncapabilities = [\"registries\"]\n";
        let with = |list: &str| Manifest::parse(&format!("{content}namespaces = [{list}]\n"));
        let m = with("\"kaisyn\", \"towns_and_towers\"").unwrap();
        let owned: Vec<&str> = m.owned_namespaces().collect();
        assert_eq!(owned, ["t_and_t", "kaisyn", "towns_and_towers"]);
        for reserved in ["minecraft", "c", "horizon"] {
            let e = with(&format!("\"{reserved}\"")).unwrap_err();
            assert!(e.contains("not a mod's to claim"), "{e}");
        }
        assert!(with("\"t_and_t\"").unwrap_err().contains("twice"));
        assert!(with("\"a\", \"a\"").unwrap_err().contains("twice"));
        assert!(with("\"Kaisyn\"").unwrap_err().contains("is not"));
        assert!(with("\"\"").unwrap_err().contains("is not"));
        let code = format!("{EXAMPLE}namespaces = [\"kaisyn\"]\n");
        assert!(Manifest::parse(&code).unwrap_err().contains("registries"));
    }

    #[test]
    fn hmod_round_trips() {
        let dir = std::env::temp_dir().join(format!("horizon-mods-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("coords_hud.hmod");
        std::fs::write(&path, Package::write(EXAMPLE, b"\0asm").unwrap()).unwrap();
        let p = Package::read(&path).unwrap();
        assert_eq!(p.manifest.id, "coords_hud");
        assert_eq!(p.wasm.as_deref(), Some(&b"\0asm"[..]));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn multiblocks_need_their_anchor_and_the_world() {
        let base = "id = \"m\"
name = \"M\"
version = \"1\"
api = \"0.3\"
side = \"server\"
";
        let parse = |caps: &str, extra: &str| {
            Manifest::parse(&format!(
                "{base}capabilities = [{caps}]
{extra}"
            ))
        };
        let both = parse(
            "\"block-entities\", \"world\", \"multiblocks\"",
            "experimental = [\"block-entities\", \"multiblocks\"]
",
        )
        .unwrap();
        assert!(both.capabilities.contains(&Capability::Multiblocks));
        for caps in [
            "\"multiblocks\"",
            "\"world\", \"multiblocks\"",
            "\"block-entities\", \"multiblocks\"",
        ] {
            let e = parse(
                caps,
                "experimental = [\"multiblocks\"]
",
            )
            .unwrap_err();
            assert!(
                e.contains("multiblocks needs capabilities block-entities and world"),
                "{e}"
            );
        }
    }

    #[test]
    fn experimental_capabilities_need_the_opt_in() {
        let base = "id = \"m\"\nname = \"M\"\nversion = \"1\"\napi = \"0.1\"\n";
        let err = Manifest::parse(&format!("{base}capabilities = [\"ui\"]\n")).unwrap_err();
        assert!(err.contains("experimental = [\"ui\"]"), "{err}");
        let m = Manifest::parse(&format!(
            "{base}capabilities = [\"ui\"]\nexperimental = [\"ui\"]\n"
        ))
        .unwrap();
        assert_eq!(m.experimental, [Capability::Ui]);
        // A capability that is (or became) stable is fine in the list.
        Manifest::parse(&format!(
            "{base}capabilities = [\"hud\"]\nexperimental = [\"hud\"]\n"
        ))
        .unwrap();
    }

    #[test]
    fn library_mods_list_what_they_provide_and_need() {
        let base = "id = \"spin_bell\"\nname = \"Bell\"\nversion = \"1\"\napi = \"0.3\"\nside = \"server\"\n";
        let parse = |caps: &str, extra: &str| {
            Manifest::parse(&format!(
                "{base}capabilities = [{caps}]\nexperimental = [\"inter-mod\"]\n{extra}"
            ))
        };
        let m = parse(
            "\"inter-mod\", \"players\"",
            "provides = [\"spin-lib@1.0\"]\nrequires = [\"spin-lib@1\"]\n",
        )
        .unwrap();
        assert_eq!(m.provides, ["spin-lib@1.0"]);
        assert_eq!(m.requires, ["spin-lib@1"]);
        assert_eq!(Capability::InterMod.to_string(), "inter-mod");
        assert!(Capability::InterMod.is_experimental());
        assert!(Capability::InterMod.on_server());
        // The entries need the capability, and their shape is checked.
        for what in ["provides", "requires"] {
            let e = parse("\"players\"", &format!("{what} = [\"spin-lib@1\"]\n")).unwrap_err();
            assert!(e.contains(&format!("{what} are for library mods")), "{e}");
            for bad in ["spin-lib", "spin-lib@", "spin-lib@1.2.3.4", "spin-lib@x"] {
                let e = parse(
                    "\"inter-mod\", \"players\"",
                    &format!("{what} = [\"{bad}\"]\n"),
                )
                .unwrap_err();
                assert!(e.contains("is not"), "{bad}: {e}");
            }
            let e = parse(
                "\"inter-mod\", \"players\"",
                &format!("{what} = [\"spin-lib@1\", \"spin-lib@1\"]\n"),
            )
            .unwrap_err();
            assert!(e.contains("twice"), "{e}");
        }
        // A name of the wrong characters, and a version past patch.
        let e = parse("\"inter-mod\", \"players\"", "provides = [\"Spin@1\"]\n").unwrap_err();
        assert!(e.contains("is not [a-z0-9_.-]"), "{e}");
        // `serves`: same name, same major, at least the asked version.
        assert!(serves("spin-lib@1.0", "spin-lib@1"));
        assert!(serves("spin-lib@1.2.5", "spin-lib@1.2"));
        assert!(serves("spin-lib@1.0", "spin-lib@1.0"));
        assert!(!serves("spin-lib@2.0", "spin-lib@1"));
        assert!(!serves("spin-lib@1.0", "spin-lib@1.2"));
        assert!(!serves("other@1.0", "spin-lib@1"));
    }
}
