<!-- Generated from crates/horizon-mod-sdk/wit/horizon.wit and the manifest's capabilities by crates/horizon-mods/tests/api_reference.rs; a test fails when this file is out of date. Update it with: HORIZON_WRITE_DOCS=1 cargo test -p horizon-mods --features pack --test api_reference -->
# Mod API reference

The contract between Horizon and its mods: the WIT package `horizon:mods@0.1.0`, every interface, type, function and hook in it, and the capabilities a manifest may list. The guide is [mods.md](../mods.md); stability and versions: [stability.md](stability.md). Manifest `api` lines this game serves: `0.1`, `0.2`, `0.3`.

## Capabilities

| In `capabilities` | Grants | The player reads "May …" |
| --- | --- | --- |
| `game-read` | [`game`](#game) | see your position, health and the time |
| `chat` | [`chat`](#chat) | show you chat messages |
| `hud` | [`hud`](#hud) | draw text on your screen |
| `storage` | [`storage`](#storage) | save its own settings to disk |
| `command` | [`command`](#command) | run commands as you, with your permissions |
| `registries` | data files (`assets/`, `data/`), no interface | add blocks and items to your worlds |
| `ui` | [`ui`](#ui) | show its own screens and a settings page |
| `render` | [`render`](#render) | draw on your screen, shake and zoom the camera |
| `input` | [`input`](#input) | see the keys you hold and your clicks, take over the use button on its own items and the mouse wheel, and add keys |
| `players` | [`players`](#players) | see who is on the server and message them |
| `world` | [`world`](#world) | read and change the world's blocks |
| `recipes` | [`recipes`](#recipes) | read the game's recipes and items and which recipes you know, place recipes in your crafting grid and, in creative mode, give you items |
| `block-entities` | [`block-entities`](#block-entities) | keep its own data on the blocks it claims, saved with the world |
| `energy` | [`energy`](#energy) | store energy in its blocks and move energy between any mod's blocks |
| `menus` | [`menus`](#menus) | open menus of its own for you, and see what you click in them |
| `displays` | [`displays`](#displays) | show block models, items and text of its own in the world, moved smoothly or riding its mobs |
| `items` | [`items`](#items) | keep items in its blocks and move items between any mod's blocks |
| `machine-recipes` | [`machine-recipes`](#machine-recipes) | read the recipes of its own machines from data files |
| `multiblocks` | [`multiblocks`](#multiblocks) | keep multiblock structures of its own: read the world's blocks to check them, and hear when they come together and break |
| `saved-data` | [`saved-data`](#saved-data) | keep data of its own on the world, saved with the world |
| `item-hooks` | [`item-hooks`](#item-hooks) | act on the items it claims: their use and their blow, each tick held or worn, and data of its own on each stack |
| `network` | [`network`](#network) | send messages to the other half of the mod, on the server or on your game, and hear its messages |
| `entities` | [`entities`](#entities) | spawn mobs of its own that it moves, hurts and removes, and hear when they are struck, used and die |
| `fluids` | [`fluids`](#fluids) | store fluids in its blocks and move fluids between any mod's blocks |
| `redstone` | [`redstone`](#redstone) | read the redstone signals around its blocks and give redstone signals out of them |
| `effects` | [`effects`](#effects) | show particles and play sounds in the world for the players near them |
| `game-events` | [`game-events`](#game-events) | be told when players are hurt, die, respawn or change dimension or game mode, when they kill mobs, and when the weather turns |
| `config` | [`config`](#config) | keep settings in a file you can edit while the game runs, and read them |
| `inter-mod` | [`inter-mod`](#inter-mod) | send messages to other server mods, and hear theirs |

## World `horizon-mod`

A client mod: runs in the player's game.

- Imports (what the game offers): `log`, `game`, `chat`, `hud`, `storage`, `command`
- Exports (what the mod provides): `events`

## World `horizon-server-mod`

A server mod (manifest `side = "server"`, `api = "0.3"`, experimental): runs where the world is simulated, on the dedicated server, a LAN host and single player's own server.

- Imports (what the game offers): `log`, `storage`, `players`, `world`
- Exports (what the mod provides): `server-events`

## World `horizon-block-mod`

A server mod with block entities (capability `block-entities`): the SDK builds every server mod against this world. Mods built for `horizon-server-mod` alone still load and simply keep none.

- Imports (what the game offers): `block-entities`, `energy`, `fluids`, `menus`, `displays`, `items`, `machine-recipes`, `multiblocks`, `saved-data`, `item-hooks`, `network`, `entities`, `redstone`, `effects`, `config`, `inter-mod`, `log`, `storage`, `players`, `world`
- Exports (what the mod provides): `block-entity-events`, `menu-events`, `block-use-events`, `multiblock-events`, `item-events`, `network-events`, `entity-events`, `redstone-events`, `game-events`, `config-events`, `inter-mod-events`, `server-events`

## World `block-entity-host`

The game's side of `horizon-block-mod` beyond `horizon-server-mod`.

- Imports (what the game offers): `block-entities`, `energy`, `fluids`, `items`
- Exports (what the mod provides): `block-entity-events`

## World `use-host`

The game's side of `block-use-events`: a world of its own, so server mods built before it still load.

- Imports (what the game offers): `players`
- Exports (what the mod provides): `block-use-events`

## World `config-host`

The game's side of capability `config`: a world of its own, so server mods built before it (no `config-events` export) still load.

- Imports (what the game offers): `config`
- Exports (what the mod provides): `config-events`

## World `game-events-host`

The game's side of capability `game-events`: a world of its own, so server mods built before it (no `game-events` export) still load.

- Imports (what the game offers): `players`
- Exports (what the mod provides): `game-events`

## World `effects-host`

The game's side of capability `effects` (imports only).

- Imports (what the game offers): `effects`
- Exports (what the mod provides): 

## World `display-host`

The game's side of capability `displays` (imports only).

- Imports (what the game offers): `displays`
- Exports (what the mod provides): 

## World `menu-host`

The game's side of capability `menus`: a world of its own, so server mods built before it (no `menu-events` export) still load.

- Imports (what the game offers): `players`, `menus`
- Exports (what the mod provides): `menu-events`

## World `machine-recipes-host`

The game's side of capability `machine-recipes` (imports only).

- Imports (what the game offers): `machine-recipes`
- Exports (what the mod provides): 

## World `multiblock-host`

The game's side of capability `multiblocks`: a world of its own, so server mods built before it (no `multiblock-events` export) still load.

- Imports (what the game offers): `multiblocks`
- Exports (what the mod provides): `multiblock-events`

## World `saved-data-host`

The game's side of capability `saved-data` (imports only).

- Imports (what the game offers): `saved-data`
- Exports (what the mod provides): 

## World `item-host`

The game's side of capability `item-hooks`: a world of its own, so server mods built before it (no `item-events` export) still load.

- Imports (what the game offers): `players`, `item-hooks`
- Exports (what the mod provides): `item-events`

## World `entity-host`

The game's side of capability `entities`: a world of its own, so server mods built before it (no `entity-events` export) still load.

- Imports (what the game offers): `players`, `entities`
- Exports (what the mod provides): `entity-events`

## World `redstone-host`

The game's side of capability `redstone`: a world of its own, so server mods built before it (no `redstone-events` export) still load.

- Imports (what the game offers): `redstone`
- Exports (what the mod provides): `redstone-events`

## World `net-server-host`

The game's side of capability `network` for server mods: a world of its own, so server mods built before it (no `network-events` export) still load.

- Imports (what the game offers): `players`, `network`
- Exports (what the mod provides): `network-events`

## World `net-client-host`

The game's side of capability `network` for client mods: a world of its own, so client mods built before it still load.

- Imports (what the game offers): `network`
- Exports (what the mod provides): `network-client-events`

## World `inter-mod-host`

The game's side of capability `inter-mod`: a world of its own, so server mods built before it (no `inter-mod-events` export) still load.

- Imports (what the game offers): `inter-mod`
- Exports (what the mod provides): `inter-mod-events`

## World `horizon-ui-mod`

A mod with widgets (capability `ui`): the SDK builds every mod against this world. Mods built for `horizon-mod` alone (no `ui-events` export) still load and simply hear nothing of their widgets.

- Imports (what the game offers): `ui`, `render`, `input`, `recipes`, `browser`, `network`, `log`, `game`, `chat`, `hud`, `storage`, `command`
- Exports (what the mod provides): `ui-events`, `network-client-events`, `events`

## World `ui-host`

The game's side of `horizon-ui-mod` beyond `horizon-mod`.

- Imports (what the game offers): `ui`
- Exports (what the mod provides): `ui-events`

## World `render-host`

The game's side of capability `render` (imports only).

- Imports (what the game offers): `render`, `input`
- Exports (what the mod provides): 

## World `recipes-host`

The game's side of capability `recipes` (imports only).

- Imports (what the game offers): `recipes`, `browser`
- Exports (what the mod provides): 

## `log`

Access: always granted.

The mod's log, in the game's log under the mod's id. Always granted.

### enum `level`

| Case | |
| --- | --- |
| `debug` |  |
| `info` |  |
| `warn` |  |
| `error` |  |

| Function | Signature | What it does |
| --- | --- | --- |
| `write` | `func(level: level, message: string)` | One line in the game's log as `[mod <id>] message`, at that level. |

## `game`

Access: capability `game-read`.

Capability `game-read`: the local player, the world clock and the block the player looks at.

### record `player`

| Field | Type | |
| --- | --- | --- |
| `x` | `f64` | Feet position, in blocks. |
| `y` | `f64` |  |
| `z` | `f64` |  |
| `yaw` | `f32` | Degrees, as F3 shows them. |
| `pitch` | `f32` | Degrees: -90 looks straight up, 90 straight down. |
| `health` | `f32` | Hit points as vanilla counts them: 20 is full health without effects. |
| `food` | `u32` | 0 to 20, as the hunger bar. |
| `dimension` | `string` | `minecraft:overworld`, `minecraft:the_nether`, ... |

### record `clock`

| Field | Type | |
| --- | --- | --- |
| `game-time` | `u64` | Ticks since the world was created. |
| `day-time` | `u64` | 24000 ticks per day; 0 is 6:00 on the first day. |

### record `hit`

A block along the player's view.

| Field | Type | |
| --- | --- | --- |
| `block-x` | `s32` | The block hit. |
| `block-y` | `s32` |  |
| `block-z` | `s32` |  |
| `x` | `f64` | The point on its face where the view meets it. |
| `y` | `f64` |  |
| `z` | `f64` |  |
| `block` | `string` | `minecraft:stone`, ... |

| Function | Signature | What it does |
| --- | --- | --- |
| `get-player` | `func() -> option<player>` | None on the title screen. |
| `get-clock` | `func() -> option<clock>` | None on the title screen. |
| `raycast` | `func(distance: f32) -> option<hit>` | The first block along the player's view (fluids pass), up to `distance` blocks (at most 1024; past the loaded chunks, across the distant terrain when LOD Distance is on), as of this tick. None when nothing is hit, and on the title screen. |

## `chat`

Access: capability `chat`.

Capability `chat`: messages only the local player sees.

| Function | Signature | What it does |
| --- | --- | --- |
| `show` | `func(text: string)` | Shown in chat after the mod's name, like a system message. |

## `hud`

Access: capability `hud`.

Capability `hud`: one panel of text lines per mod, drawn by the game every frame until the mod changes or clears it (no call per frame).

### enum `corner`

| Case | |
| --- | --- |
| `top-left` |  |
| `top-right` |  |
| `bottom-left` |  |
| `bottom-right` |  |

### record `line`

| Field | Type | |
| --- | --- | --- |
| `text` | `string` | Plain text, up to 256 characters. |
| `color` | `u32` | 0xRRGGBB. |

| Function | Signature | What it does |
| --- | --- | --- |
| `set-panel` | `func(corner: corner, lines: list<line>)` | Replaces the mod's panel: up to 16 lines in that corner of the screen. |
| `clear` | `func()` | Removes the mod's panel. |

## `storage`

Access: capability `storage`.

Capability `storage`: a per-mod folder of small key/value files, quota'd (docs/mods.md). Keys are `[a-z0-9_.-]{1,64}`, one path segment.

| Function | Signature | What it does |
| --- | --- | --- |
| `get` | `func(key: string) -> option<list<u8>>` | None if `key` has never been `set`, or the key is not well-formed. |
| `set` | `func(key: string, value: list<u8>) -> result<_, string>` | Errs on a malformed key or over quota (bytes for the whole mod). |
| `delete` | `func(key: string)` | No error if `key` was already absent. |

## `command`

Access: capability `command`.

Capability `command`: vanilla commands run as the player, with the player's permissions, after the hook call returns (8 per call). A mod's own commands are not run this way.

| Function | Signature | What it does |
| --- | --- | --- |
| `run` | `func(line: string)` | `line` without its slash; its feedback goes to the chat as if typed. |
| `run-quiet` | `func(lines: list<string>)` | `lines` (each without its slash, at most 256 a call) as `run` does, but with nothing in the chat: no feedback and no failure. In single player and on Horizon's servers, each runs exactly as the player typing it would (the same rights), in order, 64 a tick, and the server logs one line a batch; one that carries a chat message (`say`, `msg`) is refused. On any other server they go as chat commands, and there their feedback shows: a mod running many should say so. |

## `ui`

Access: capability `ui`.

Capability `ui`: widgets the game draws with its own menus' widgets, for a screen the mod owns and a settings page in Options > Mods. Layout and drawing are the game's: a mod says what its widgets hold, never where they sit or how they look, and the game clips everything it is handed.

### record `text-widget`

A line of text.

| Field | Type | |
| --- | --- | --- |
| `id` | `u32` | The mod's own name for the widget; `ui-events` calls echo it. |
| `text` | `string` | Up to 256 characters. |
| `color` | `u32` | 0xRRGGBB; 0 is the menus' white. |

### record `item-widget`

An item's icon and a name beside it, as recipe viewers show them.

| Field | Type | |
| --- | --- | --- |
| `id` | `u32` | The mod's own name for the widget. |
| `item` | `string` | `minecraft:diamond`, `<mod>:<path>`; an unknown id draws a "?". |
| `name` | `string` | The name shown beside the icon (empty: the item's own). |

### record `list-widget`

Rows the player picks one of, as an options page lists choices: a pick is `ui-events::select`, `selected` is the row to mark now.

| Field | Type | |
| --- | --- | --- |
| `id` | `u32` | The mod's own name for the widget; `ui-events::select` echoes it. |
| `rows` | `list<string>` | The rows' texts, at most 512, each up to 256 characters. |
| `selected` | `option<u32>` | The row to mark as the current choice, if any. |

### record `button-widget`

A thing on a mod's screen or settings page. A button as the options pages have them (200 × 20): a click is `ui-events::select(id, 0)`. Mods cycle its label for a choice ("Corner: Top Left").

| Field | Type | |
| --- | --- | --- |
| `id` | `u32` | The mod's own name for the widget. |
| `label` | `string` | Up to 256 characters. |

### variant `widget`

| Case | Payload | |
| --- | --- | --- |
| `text` | `text-widget` | Text the player reads. |
| `button` | `button-widget` | A button. |
| `item` | `item-widget` | An item's icon and name. |
| `list` | `list-widget` | Rows the player picks from. |

### record `screen`

A screen the mod owns: a title over widgets stacked down the middle, the Done button the game adds, and Esc to leave it.

| Field | Type | |
| --- | --- | --- |
| `title` | `string` | Its title, up to 256 characters. |
| `widgets` | `list<widget>` | Its widgets, in order, at most 64. |

| Function | Signature | What it does |
| --- | --- | --- |
| `open-screen` | `func(screen: screen)` | Show `screen` over the game; a second call while it is open replaces it. Only the last call of one hook counts. |
| `close-screen` | `func()` | Close the mod's screen when it is open (the mod hears no `closed` for this: it was its own doing). |
| `set-settings` | `func(widgets: list<widget>)` | The mod's page in Options > Mods, reached from a settings button on its row: the widgets it shows, an empty list for no page. The player opening the page is `ui-events::settings-opened`: push fresh widgets then; later calls update the page while it shows. |

## `ui-events`

Access: exported by the mod: the game calls it.

What the game calls on a mod that shows widgets (capability `ui`): what the player did with them.

| Function | Signature | What it does |
| --- | --- | --- |
| `select` | `func(id: u32, row: u32)` | A row of one of the mod's `list` widgets was picked: the widget's `id` and the row, on the screen or the settings page that showed it. |
| `closed` | `func()` | The player left the mod's screen (Esc or Done); not for a screen the mod closed itself or one that went away with the mod. |
| `settings-opened` | `func()` | The player opened the mod's settings page in Options > Mods: call `ui::set-settings` now to fill it with fresh widgets. |

## `input`

Access: capability `input`.

Capability `input` (experimental: the manifest needs `experimental = ["input"]`): the use button on items the mod claims, and the mod's own keys. Events wait for `poll`, at most 256.

### record `held-item`

The main hand's stack.

| Field | Type | |
| --- | --- | --- |
| `id` | `string` | `minecraft:spyglass`, `<mod>:<path>`. |
| `count` | `u32` |  |
| `slot` | `u32` | The hotbar slot, 0 to 8. |

### record `item-use`

| Field | Type | |
| --- | --- | --- |
| `item` | `held-item` |  |
| `ticks` | `u32` | Game ticks the button was held (0 on `use-start`). |

### variant `event`

| Case | Payload | |
| --- | --- | --- |
| `use-start` | `item-use` | The use button went down with a claimed item in the main hand. The game does nothing else with the click: no placing, no eating, no swing. |
| `use-stop` | `item-use` | It came up again, or the item left the hand, or a screen opened. |
| `key-down` | `string` | A key `register-key` named went down or up while playing (no screen open). The game's own mapping on that key still runs. A screen opening lets every held key up. |
| `key-up` | `string` |  |
| `wheel` | `f32` | Mouse wheel steps while the mod holds the wheel (`claim-wheel`): positive away from the player, as the hotbar reads them. |

### record `mapping-state`

A mapping as the player has bound it.

| Field | Type | |
| --- | --- | --- |
| `label` | `string` | The bound key's name as Controls shows it: `W`, `Space`, `LMB`. |
| `down` | `bool` | Held down now (as of this frame for a mod with a `frame` hook, else this tick). |

| Function | Signature | What it does |
| --- | --- | --- |
| `claim-items` | `func(ids: list<string>)` | The items whose use the mod handles instead of the game, by id (at most 64). Replaces the previous list. |
| `register-key` | `func(name: string, label: string, key: string) -> result<_, string>` | A key of the mod's: `name` comes back in `key-down` and `key-up`, `label` says what it does, `key` is the key as options.txt writes it (`key.keyboard.r`), the default: the player can bind another one on the mod's page in Options > Mods, and that one wins. At most 16; an unknown key is an error. |
| `held` | `func() -> option<held-item>` | The main hand's stack, as of this tick; none when it is empty. |
| `poll` | `func() -> list<event>` | The events since the last call, oldest first. |
| `claim-wheel` | `func(on: bool)` | Takes the mouse wheel while playing (true) or gives it back: while taken, its steps are the mod's `wheel` events and the hotbar doesn't move. The game gives it back when the mod stops. |
| `mapping` | `func(name: string) -> option<mapping-state>` | A movement or click mapping as the player has bound it: `key.forward`, `key.left`, `key.back`, `key.right`, `key.jump`, `key.sneak`, `key.sprint`, `key.attack` or `key.use`. None for another name. |
| `clicks-per-second` | `func() -> tuple<u32, u32>` | Clicks of the attack and use buttons in the last second. |

## `render`

Access: capability `render`.

Capability `render` (experimental: the manifest needs `experimental = ["render"]`): shapes the game draws for the mod with its own renderer, kept until the mod replaces them (no call per frame). A mod never supplies shader code.

### enum `corner`

A corner of the screen, as `hud`'s panels use them.

| Case | |
| --- | --- |
| `top-left` |  |
| `top-right` |  |
| `bottom-left` |  |
| `bottom-right` |  |

### record `rect`

A rectangle in GUI pixels: the top left corner and the size.

| Field | Type | |
| --- | --- | --- |
| `x` | `f32` |  |
| `y` | `f32` |  |
| `w` | `f32` |  |
| `h` | `f32` |  |

### record `point`

A point in GUI pixels from the screen's top left.

| Field | Type | |
| --- | --- | --- |
| `x` | `f32` |  |
| `y` | `f32` |  |

### record `fill-shape`

| Field | Type | |
| --- | --- | --- |
| `rect` | `rect` |  |
| `color` | `u32` | 0xAARRGGBB. |

### record `quad-shape`

Any four-cornered shape (a wheel's segment, an arrow): the corners in order round it.

| Field | Type | |
| --- | --- | --- |
| `corners` | `tuple<point, point, point, point>` |  |
| `color` | `u32` | 0xAARRGGBB. |

### record `sprite-shape`

| Field | Type | |
| --- | --- | --- |
| `sprite` | `string` | One of the game's GUI sprites, as `gui/sprites/` names them (`hud/heart/full`, `widget/button`), the resource packs' versions included. An unknown one draws nothing. |
| `rect` | `rect` |  |
| `color` | `u32` | A tint, 0xAARRGGBB: 0xFFFFFFFF draws it as it is. |

### record `item-shape`

| Field | Type | |
| --- | --- | --- |
| `item` | `string` | `minecraft:diamond`, `<mod>:<path>`; an unknown one draws nothing. |
| `at` | `point` | The icon's top left. |
| `scale` | `f32` | 1.0 is the hotbar's 16 × 16. |

### record `text-shape`

| Field | Type | |
| --- | --- | --- |
| `text` | `string` | Up to 256 characters, one line. |
| `at` | `point` |  |
| `color` | `u32` | 0xRRGGBB, as `hud`'s lines. |
| `centered` | `bool` | Centred on `at.x` instead of starting there. |
| `shadow` | `bool` |  |

### variant `shape`

| Case | Payload | |
| --- | --- | --- |
| `fill` | `fill-shape` |  |
| `quad` | `quad-shape` |  |
| `sprite` | `sprite-shape` |  |
| `item` | `item-shape` |  |
| `text` | `text-shape` |  |

### record `frame-rate`

| Field | Type | |
| --- | --- | --- |
| `fps` | `u32` | Frames drawn in the last second. |
| `min` | `u32` | The slowest frame's rate, and the 1% low (the 99th percentile frame time's rate), over the last 5 seconds. |
| `low` | `u32` |  |

### record `zoom`

| Field | Type | |
| --- | --- | --- |
| `factor` | `f32` | The field of view is divided by it: 1 is none, at most 10. |
| `smooth` | `bool` | Ease in and out through the game's field-of-view easing, as the spyglass does, instead of at once. |
| `turn` | `f32` | Mouse turning is multiplied by it while zoomed: 1 keeps the player's sensitivity, at least 0.05. |

### record `minimap`

| Field | Type | |
| --- | --- | --- |
| `corner` | `corner` |  |
| `size` | `f32` | Its side in GUI pixels, 16 to 256. |
| `blocks-per-pixel` | `f32` | World blocks per map pixel, 0.5 to 4; the map is 128 × 128 pixels. |
| `rotate` | `bool` | The map turns with the player, who faces up, and an N on its rim marks north. Otherwise north is up and the player's arrow turns. |
| `round` | `bool` | Round, the corners left clear; otherwise square. |

### record `far-terrain`

| Field | Type | |
| --- | --- | --- |
| `distance` | `u32` | How far the distant terrain reaches, in chunks: 64 to 1024. On integrated graphics other than Apple's the game caps it lower. |
| `reduced` | `bool` | Coarser far tiles, as the Far Detail setting's Reduced: 2 to 4 ms less GPU time per frame on integrated graphics. |

| Function | Signature | What it does |
| --- | --- | --- |
| `set-hud` | `func(shapes: list<shape>)` | Replaces the mod's HUD shapes: up to 1024, drawn every frame in order over the HUD (after the hotbar, under chat and screens), hidden with the HUD (F1). An empty list removes them. |
| `gui-size` | `func() -> tuple<f32, f32>` | The screen's size in GUI pixels (width, height), as of this tick. |
| `shake` | `func(strength: f32, seconds: f32)` | Shakes the camera: a jolt of `strength` (0 to 1, 1 the hardest) that fades out over `seconds` (at most 10). Jolts add up to 1. The view turns, the player doesn't, and the player's Distortion Effects option scales it (0 turns it off). |
| `set-corner` | `func(corner: corner, width: f32, height: f32, shapes: list<shape>)` | Replaces the mod's corner panel: shapes in GUI pixels from the top left of a `width` × `height` box that the game places in `corner`, stacked with the game's overlays and the other mods' panels (under the status effects at the top right), as `hud`'s panels are. |
| `clear-corner` | `func()` | Removes the mod's corner panel. |
| `text-width` | `func(text: string) -> f32` | The width of one line of text in GUI pixels, in the HUD's font: exact for Latin letters (U+0000 to U+024F), 6 per other character. |
| `frame-stats` | `func() -> frame-rate` | The game's frame counter, recomputed twice a second. |
| `set-zoom` | `func(zoom: zoom)` | Zooms the camera until the next call (`factor` 1 ends it), only while playing: a screen opening or the spyglass's own view (which wins) leave the camera vanilla's. With several mods, the strongest zoom counts. |
| `set-minimap` | `func(minimap: option<minimap>)` | Shows a map of the world around the player in `corner`, stacked with the game's overlays and the mods' panels, until the next call (none hides it). It looks like a map item: vanilla's map colours with their height shading and water depth, and the player's arrow. Underground, under a roof and in the Nether it maps the floor around the player's height. The game keeps the map's tiles and refreshes them within its own frame budget; hidden, they are dropped. One map shows at a time: the first loaded mod's. |
| `set-far-terrain` | `func(far-terrain: option<far-terrain>)` | Draws the world beyond the render distance as distant terrain, out to `distance` chunks, until the next call (none ends it). The game builds it on its own workers, from the chunks already loaded or saved and, where nobody has been, from the world's generator; the simulation distance doesn't change. Only worlds whose generator the game has (single player and a LAN world you host) show it. The player's own LOD Distance setting, when on, wins. One far terrain shows at a time: the first loaded mod's. |

## `events`

Access: exported by the mod: the game calls it.

What the game calls. Only the hooks `init` asks for are ever called.

### record `registration`

| Field | Type | |
| --- | --- | --- |
| `commands` | `list<string>` | Chat commands, without the slash. A name vanilla already uses is refused (vanilla's command runs). |
| `tick` | `bool` | Call `tick` every game tick (20 per second while playing). |
| `frame` | `bool` | Call `frame` every rendered frame. Budget: 0.1 ms. |
| `chat` | `bool` | Call `chat` for each chat line the player receives. |

| Function | Signature | What it does |
| --- | --- | --- |
| `init` | `func() -> result<registration, string>` | Once, after loading. An error keeps the mod off and is shown. |
| `tick` | `func(tick: u64)` | Each game tick while in a world, with the game's tick counter. |
| `frame` | `func(seconds: f32)` | Each rendered frame, with the seconds since the last one. |
| `command` | `func(name: string, args: string) -> result<option<string>, string>` | `/name args`: ok(some(text)) is shown as feedback, err(text) in red. |
| `chat` | `func(text: string)` | Each chat line the player receives, as plain text. |

## `recipes`

Access: capability `recipes`.

Capability `recipes` (experimental): the recipes and items the game knows, for recipe viewers. In single player and on Horizon's servers they are the world's; on any other server they are vanilla 26.3's (a client is sent no others), while `known` follows that server's book.

### record `stack`

An item of `items`, by its index there, with a count and the components it has beyond the item's own, in vanilla's item syntax (`[potion_contents={potion:"minecraft:swiftness"}]`), or "".

| Field | Type | |
| --- | --- | --- |
| `item` | `u32` |  |
| `count` | `u32` |  |
| `components` | `string` |  |

### enum `kind`

The station a recipe is made at.

| Case | |
| --- | --- |
| `crafting-shaped` |  |
| `crafting-shapeless` |  |
| `smelting` |  |
| `blasting` |  |
| `smoking` |  |
| `campfire` |  |
| `stonecutting` |  |
| `smithing` |  |
| `brewing` |  |

### record `recipe`

| Field | Type | |
| --- | --- | --- |
| `id` | `string` | `minecraft:oak_planks`. |
| `kind` | `kind` |  |
| `category` | `string` | The recipe book's tab (`crafting_building_blocks`, `furnace_food`), or "". |
| `group` | `string` | Recipes the book shows on one button share a group, or "". |
| `width` | `u32` | A shaped recipe's grid: `slots` holds its cells row by row. 0 for the other kinds. |
| `height` | `u32` |  |
| `slots` | `list<list<stack>>` | Each slot's stacks, any one of which fits; an empty list is an empty cell. Cooking and stonecutting: the input. Smithing: the template, the base, the addition. Brewing: the bottle, the reagent. |
| `results` | `list<stack>` | What it makes; several when that varies (a trim on any armour). |
| `cooking-time` | `u32` | Cooking: game ticks and experience; 0 for the other kinds. |
| `experience` | `f32` |  |
| `book-id` | `s32` | The recipe book's id for it (one per way the book shows it), as `known` lists them; -1 for the kinds the book doesn't show (campfire, stonecutting, smithing, brewing). |

### record `item`

| Field | Type | |
| --- | --- | --- |
| `id` | `string` | `minecraft:diamond`, `<mod>:<path>`. |
| `name` | `string` | In the player's language. |
| `tags` | `list<string>` | Its item tags, without the `#`. |

| Function | Signature | What it does |
| --- | --- | --- |
| `items` | `func() -> list<item>` | Every item, in the registry's order: `stack.item` indexes this list. |
| `all` | `func() -> list<recipe>` | Every recipe: the book's in its order, then campfire, stonecutting, smithing and brewing. They don't change while a world is open. |
| `known` | `func() -> list<s32>` | The `book-id`s of the recipes the player's recipe book holds now. |
| `place` | `func(book-id: s32, all: bool) -> result<_, string>` | Places a recipe in the open screen's grid as a click in the recipe book does (`ServerboundPlaceRecipePacket`): the inventory's grid or a crafting table. `all` places as many as the items allow, as with Shift. Refused while no such screen is open, for a recipe the player's book doesn't hold and for one that can't be placed; one missing items shows as a ghost in the grid, as in the book. |
| `give` | `func(stack: stack) -> result<_, string>` | Creative mode only: gives the player the stack, as taking it from the creative inventory does (`ServerboundSetCreativeModeSlot`): the selected slot first, then the hotbar, then the inventory. The count is cut to one stack of the item (64, 16 or 1). |

## `browser`

Access: not granted by any capability.

Capability `ui` (experimental): a recipe viewer's panel beside the game's inventory and container screens (not the creative inventory), on the right of the screen. The game draws it with its own widgets and slots, pages it and keeps its search box; the mod says what it holds. The first loaded mod with a panel shows.

### record `stack`

An item to show: its id, a count (1 draws none) and its components beyond the item's own, in vanilla's item syntax, or "".

| Field | Type | |
| --- | --- | --- |
| `item` | `string` |  |
| `count` | `u32` |  |
| `components` | `string` |  |

### record `click`

A click on an item of the panel, by its index in `set-items`'s list.

| Field | Type | |
| --- | --- | --- |
| `index` | `u32` |  |
| `right` | `bool` |  |

### record `item-key`

A key the mod registered (`input.register-key`) went down while the pointer was on an item: a slot of the screen's, or the panel's.

| Field | Type | |
| --- | --- | --- |
| `key` | `string` |  |
| `stack` | `stack` |  |

### record `slot-click`

A click on a slot of a recipe the panel shows (`set-recipes`).

| Field | Type | |
| --- | --- | --- |
| `recipe` | `u32` | The recipe's index in `set-recipes`'s list and the slot's in it; the results come after the slots. |
| `slot` | `u32` |  |
| `stack` | `stack` | The stack the slot showed at that moment. |
| `right` | `bool` |  |

### variant `event`

| Case | Payload | |
| --- | --- | --- |
| `clicked` | `click` |  |
| `search` | `string` | The search box's text, each time the player changes it. |
| `key` | `item-key` |  |
| `slot-clicked` | `slot-click` |  |
| `action` | `u32` | The action button of the recipe at this index in `set-recipes`'s list. |
| `page` | `u32` | The player turned the recipes' page (`set-recipes`): the index of the first recipe now shown. |
| `back` |  | The back button of `set-recipes`. |
| `tab` | `u32` | The player picked the tab at this index of `set-recipes`' tabs. |

### enum `layout`

How the game lays out a recipe of `set-recipes`.

| Case | |
| --- | --- |
| `grid` | A crafting grid of `width` x `height` slots, an arrow and the result. |
| `cooking` | One input over a flame, an arrow and the result (furnaces, smokers, campfires). |
| `single` | One input, an arrow and the result (the stonecutter). |
| `smithing` | Template, base and addition, an arrow and the result. |
| `brewing` | A potion and an ingredient, an arrow and the result. |

### record `recipe-view`

A recipe to show. Each slot, and the result, cycles through its stacks, one a second; an empty slot shows nothing.

| Field | Type | |
| --- | --- | --- |
| `layout` | `layout` |  |
| `width` | `u32` | The grid's size (`grid` only), at most 3 x 3. |
| `height` | `u32` |  |
| `slots` | `list<list<stack>>` |  |
| `results` | `list<stack>` |  |
| `note` | `string` | A short line under the recipe (a cooking time), or "". |
| `action` | `option<bool>` | The action button beside it: none, enabled, or shown greyed. |

| Function | Signature | What it does |
| --- | --- | --- |
| `set-items` | `func(items: list<stack>)` | The panel's items, at most 8192 (an empty list hides the panel). |
| `set-recipes` | `func(title: string, tabs: list<stack>, tab: u32, recipes: list<recipe-view>)` | Shows these recipes in the panel instead of its items, as many as fit a page, under `title` and a back button, with a row of `tabs` (each an icon; `tab` is the selected one) between them; an empty list of recipes shows the items again. At most 256 recipes and 16 tabs. |
| `poll` | `func() -> list<event>` | What the player did on the panel since the last poll, at most 256 kept. |

## `players`

Access: capability `players`.

Capability `players`, server mods only (manifest `api = "0.3"`, experimental): the players on the server, and messages to them.

### enum `game-mode`

| Case | |
| --- | --- |
| `survival` |  |
| `creative` |  |
| `adventure` |  |
| `spectator` |  |

### record `player`

| Field | Type | |
| --- | --- | --- |
| `id` | `string` | The player's UUID, `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`. |
| `name` | `string` |  |
| `x` | `f64` | Feet position, in blocks. |
| `y` | `f64` |  |
| `z` | `f64` |  |
| `dimension` | `string` | `minecraft:overworld`, `minecraft:the_nether`, ... |
| `game-mode` | `game-mode` |  |
| `health` | `f32` | Hit points as vanilla counts them: 20 is full health without effects. |

| Function | Signature | What it does |
| --- | --- | --- |
| `online` | `func() -> list<player>` | The players online, in the order they joined. |
| `tell` | `func(player: string, text: string) -> bool` | A system message to one player, after the mod's name; false when no player with that id is online. Up to 8 messages per call. |
| `broadcast` | `func(text: string)` | A system message to every player and the server's log, after the mod's name. Counts toward the same 8 per call. |

## `world`

Access: capability `world`.

Capability `world`, server mods only (manifest `api = "0.3"`, experimental): the blocks of the server's loaded chunks, and changes to them through the server's own block updates.

### record `block-change`

| Field | Type | |
| --- | --- | --- |
| `x` | `s32` |  |
| `y` | `s32` |  |
| `z` | `s32` |  |
| `state` | `string` | `minecraft:oak_log[axis=y]`; properties left out take the block's defaults, `minecraft:stone` alone is the default state. |

| Function | Signature | What it does |
| --- | --- | --- |
| `get-block` | `func(dimension: string, x: s32, y: s32, z: s32) -> option<string>` | The block state at a position in a dimension (`minecraft:overworld`, ...), as `minecraft:oak_log[axis=y]`, as of this call: none where the server has no chunk loaded there. |
| `get-biome` | `func(dimension: string, x: s32, y: s32, z: s32) -> option<string>` | The biome at a position (`minecraft:plains`, ...), as the server stores it (one per 4 x 4 x 4 cell): none where no chunk is loaded. |
| `set-blocks` | `func(dimension: string, blocks: list<block-change>) -> result<_, string>` | Sets blocks as `/setblock` does: the old block entity removed, neighbours and shapes updated, light, and every player told. Every state is checked now (an error queues nothing); the changes are made in order after this call returns, before that dimension's next tick; one in a chunk not loaded then is skipped. At most 4096 per call. |

## `block-entities`

Access: capability `block-entities`.

Capability `block-entities`, server mods only (manifest `api = "0.3"`, experimental): bytes the mod keeps on each placed copy of the blocks it claims (`block-entity-events.blocks`), saved with the world. A block entity starts empty when a player places one of those blocks or the mod sets one with `world.set-blocks`, and goes when its block does.

| Function | Signature | What it does |
| --- | --- | --- |
| `get` | `func(dimension: string, x: s32, y: s32, z: s32) -> option<list<u8>>` | The data of the mod's block entity at a position: none where it has none. |
| `set` | `func(dimension: string, x: s32, y: s32, z: s32, data: list<u8>) -> result<_, string>` | Replaces the data of the mod's block entity at a position: an error where it has none, or for more than 16 KiB. |

## `energy`

Access: capability `energy`.

Capability `energy`, server mods only (manifest `api = "0.3"`, experimental): energy buffers on block entities, shared by every mod. A mod gives its own block entities buffers; any mod fills and drains any buffer, so one mod's generator feeds another's machine. Buffers are saved with the world and go with their block entity (after its `removed`).

### record `storage`

| Field | Type | |
| --- | --- | --- |
| `stored` | `u64` |  |
| `capacity` | `u64` |  |
| `max-receive` | `u64` | The most `insert` puts in, and `extract` takes out, per call. |
| `max-extract` | `u64` |  |

| Function | Signature | What it does |
| --- | --- | --- |
| `get` | `func(dimension: string, x: s32, y: s32, z: s32) -> option<storage>` | The buffer at a position, whichever mod keeps it: none where there is none. |
| `set-storage` | `func(dimension: string, x: s32, y: s32, z: s32, capacity: u64, max-receive: u64, max-extract: u64) -> result<_, string>` | Gives the mod's own block entity at a position a buffer, or changes its limits; what it stores is kept up to the new capacity. An error where the mod has no block entity, or another mod keeps a buffer. |
| `set-stored` | `func(dimension: string, x: s32, y: s32, z: s32, stored: u64) -> result<_, string>` | Sets what the mod's own buffer stores (a generator making energy, a machine using it), up to its capacity. An error where the mod keeps no buffer. |
| `insert` | `func(dimension: string, x: s32, y: s32, z: s32, amount: u64, simulate: bool) -> u64` | Puts up to `amount` in the buffer at a position, as far as its `max-receive` and room allow: what went in (0 where there is none). `simulate`: only says. |
| `extract` | `func(dimension: string, x: s32, y: s32, z: s32, amount: u64, simulate: bool) -> u64` | Takes up to `amount` from the buffer at a position, as far as its `max-extract` and what it stores allow: what came out. |
| `push` | `func(dimension: string, x: s32, y: s32, z: s32, amount: u64) -> u64` | Moves up to `amount` from the mod's own buffer at a position into the buffers on its six sides, shared evenly, each as far as it takes: what moved. |

## `fluids`

Access: capability `fluids`.

Capability `fluids`, server mods only (manifest `api = "0.3"`, experimental): fluid tanks on block entities, shared by every mod (as energy is). A mod gives its own block entities up to eight tanks; any mod puts fluid in and takes it out, so one mod's pump feeds another's machine, and `push` moves fluid between neighbouring blocks like a pipe. Amounts are in millibuckets (a bucket is 1000). A tank holds one fluid at a time, named `namespace:path`: a vanilla one (`minecraft:water`) or the mod's own (`cells:plasma`; the game has no list of those, so only the name is checked). Tanks are saved with the world and go with their block entity (after its `removed`, which can still read them). Putting a vanilla fluid in the world and taking it out is the `world` capability's job: read the block, `set-blocks` it away, then `insert`.

### record `tank`

| Field | Type | |
| --- | --- | --- |
| `fluid` | `string` | `minecraft:water`; empty while the tank is. |
| `amount` | `u64` |  |
| `capacity` | `u64` |  |
| `max-receive` | `u64` | The most `insert` puts in, and `extract` takes out, per call. |
| `max-extract` | `u64` |  |

### record `tank-spec`

What a tank is made as.

| Field | Type | |
| --- | --- | --- |
| `capacity` | `u64` |  |
| `max-receive` | `u64` |  |
| `max-extract` | `u64` |  |

### record `fluid-stack`

| Field | Type | |
| --- | --- | --- |
| `fluid` | `string` |  |
| `amount` | `u64` |  |

| Function | Signature | What it does |
| --- | --- | --- |
| `set-tanks` | `func(dimension: string, x: s32, y: s32, z: s32, tanks: list<tank-spec>) -> result<_, string>` | Gives the mod's own block entity at a position tanks made as `tanks` (1 to 8, each up to 2^50 mB), or changes them: a tank that stays keeps what it holds up to its new capacity, one that goes loses it. An error where the mod has no block entity, or another mod keeps tanks. |
| `get` | `func(dimension: string, x: s32, y: s32, z: s32) -> option<list<tank>>` | The tanks at a position, whichever mod keeps them: none where there are none. |
| `set` | `func(dimension: string, x: s32, y: s32, z: s32, tank: u32, fluid: option<string>, amount: u64) -> result<_, string>` | Sets what the mod's own tank `tank` holds, up to its capacity (none or 0 empties it). An error where the mod keeps no such tank, or for a name that is not `namespace:path`. |
| `insert` | `func(dimension: string, x: s32, y: s32, z: s32, fluid: string, amount: u64, simulate: bool) -> u64` | Puts up to `amount` mB of `fluid` into the tanks at a position: into those holding it first, then into empty ones, each as far as its `max-receive` and room allow. What went in (0 where there are no tanks, or for a bad name). `simulate`: only says. |
| `extract` | `func(dimension: string, x: s32, y: s32, z: s32, tank: u32, amount: u64, simulate: bool) -> option<fluid-stack>` | Takes up to `amount` mB from tank `tank` at a position, as far as its `max-extract` and what it holds allow: what came out, none for nothing. |
| `push` | `func(dimension: string, x: s32, y: s32, z: s32, tank: u32, amount: u64) -> u64` | Moves up to `amount` mB from the mod's own tank `tank` at a position into the tanks on its six sides that take the fluid, shared evenly, each as far as it takes: what moved. |
| `fluid-of-bucket` | `func(item: string) -> option<string>` | The fluid a vanilla bucket item holds (`minecraft:water_bucket` is `minecraft:water`, 1000 mB): none for any other item. |
| `bucket-of-fluid` | `func(fluid: string) -> option<string>` | The vanilla bucket item that holds a fluid: none for any other fluid. |

## `items`

Access: capability `items`.

Capability `items`, server mods only (manifest `api = "0.3"`, experimental): inventories on block entities, shared by every mod (as energy is). A mod gives its own block entities slots; any mod puts items in and takes them out, so one mod's pipe feeds another's machine. Inventories are saved with the world; when their block goes, its items drop there (after `removed`, which can still read them).

### record `stack`

| Field | Type | |
| --- | --- | --- |
| `item` | `string` | `minecraft:coal`, ... |
| `count` | `u32` |  |
| `components` | `string` | Components beyond the item's own, in vanilla's item syntax; empty for none. |

| Function | Signature | What it does |
| --- | --- | --- |
| `set-slots` | `func(dimension: string, x: s32, y: s32, z: s32, slots: u32) -> result<_, string>` | Gives the mod's own block entity at a position an inventory of `slots` slots (at most 54), or changes how many; the items of slots it loses drop there. An error where the mod has no block entity, or another mod keeps an inventory. |
| `get` | `func(dimension: string, x: s32, y: s32, z: s32) -> option<list<option<stack>>>` | The inventory at a position, whichever mod keeps it, slot by slot: none where there is none. |
| `set` | `func(dimension: string, x: s32, y: s32, z: s32, slot: u32, stack: option<stack>) -> result<_, string>` | Replaces slot `slot` of the mod's own inventory (none empties it): an error for an unknown item or a count past its stack size. |
| `insert` | `func(dimension: string, x: s32, y: s32, z: s32, stack: stack, simulate: bool) -> u32` | Puts as much of `stack` as fits into the inventory at a position, on equal stacks first: how many went in (0 for an unknown item). `simulate`: only says. |
| `extract` | `func(dimension: string, x: s32, y: s32, z: s32, slot: u32, count: u32, simulate: bool) -> option<stack>` | Takes up to `count` items from slot `slot` of the inventory at a position: what came out. |

## `machine-recipes`

Access: capability `machine-recipes`.

Capability `machine-recipes`, server mods only (manifest `api = "0.3"`, experimental): the recipes of the mod's own machines. The game loads them once, before the mod starts, from `data/<id>/recipe/` files of the mod's id — in its own package and in a content mod's carry of the same id. It parses each file's `type` and the ingredient syntax of `inputs` and `results` (tags expanded); the whole file stays the mod's own in `data`, for the fields only it knows.

### record `stack`

| Field | Type | |
| --- | --- | --- |
| `item` | `string` | `minecraft:redstone`, `<mod>:<path>`. |
| `count` | `u32` |  |

### record `machine-recipe`

| Field | Type | |
| --- | --- | --- |
| `id` | `string` | `<id>:<path>`: the file's place under `data/<id>/recipe/`. |
| `kind` | `string` | The file's `type`, as `cells:grinding`. |
| `inputs` | `list<list<stack>>` | What goes in, slot by slot: each slot's list holds every stack that fits it (a tag's members); an empty list is a free slot. |
| `results` | `list<list<stack>>` | What comes out, as `inputs`. |
| `data` | `string` | The file's whole JSON text, for the fields only the mod knows (energy, ticks, tiers). |

| Function | Signature | What it does |
| --- | --- | --- |
| `all` | `func(kinds: list<string>) -> list<machine-recipe>` | The mod's machine recipes of these `kind`s (at most 16; an empty list: every one). They are loaded before `init` and never change while the mod runs — call once in `init` and keep, not each tick. |
| `by-input` | `func(stack: stack, kinds: list<string>) -> list<machine-recipe>` | Those of `all` a slot of whose `inputs` takes `stack`'s item: the recipes a machine holding `stack` can make now. |

## `multiblocks`

Access: capability `multiblocks`.

Capability `multiblocks`, server mods only (manifest `api = "0.3"`, experimental): the mod's multiblock structures. Templates come from its `data/<id>/multiblock/` files, loaded once before the mod starts (as `machine-recipes` are); `check` matches a template against the world at a position and a turn, and `watch` asks the game to follow a copy around one of the mod's block entities, calling `multiblock-events` when it comes together and when it breaks. Needs capabilities `block-entities` (the anchor) and `world`.

### record `cell`

| Field | Type | |
| --- | --- | --- |
| `dx` | `s32` |  |
| `dy` | `s32` |  |
| `dz` | `s32` |  |
| `blocks` | `list<string>` | Any of these block ids fits; `minecraft:air` names a cell that must be empty. Block states (properties) are not matched. |

### record `structure`

| Field | Type | |
| --- | --- | --- |
| `id` | `string` | `<id>:<path>`: the file's place under `data/<id>/multiblock/`. |
| `width` | `u32` | The shape's size in blocks: cells along x, layers along y, rows along z. |
| `height` | `u32` |  |
| `depth` | `u32` |  |
| `anchor` | `tuple<u32, u32, u32>` | The shape's cell the anchor sits on: the mod's block entity whose position a `check` takes. |
| `cells` | `list<cell>` | The shape's checked cells (`any` cells are left out): each with its offset from the anchor and the blocks that fit it. |
| `data` | `string` | The file's whole JSON text, for the fields only the mod knows. |

### record `mismatch`

| Field | Type | |
| --- | --- | --- |
| `x` | `s32` | Where in the world. |
| `y` | `s32` |  |
| `z` | `s32` |  |
| `expected` | `string` | The block the template takes there, as `minecraft:iron_block`. |
| `found` | `string` | What is there, as `minecraft:oak_log[axis=y]`. |

| Function | Signature | What it does |
| --- | --- | --- |
| `all` | `func() -> list<structure>` | The mod's structures, loaded before `init` and never changed while it runs — call once in `init` and keep. At most 64 a mod. |
| `check` | `func(dimension: string, x: s32, y: s32, z: s32, quarter: u32, id: string) -> result<option<list<mismatch>>, string>` | Matches structure `id` against the world with its anchor's block entity at a position, turned by `quarter` quarter turns clockwise looked at from above (0 to 3; y offsets don't turn). ok(none): the structure is whole; ok(some): the first mismatches, at most 16. Err for no such structure of this mod, a `quarter` past 3, no chunk of the structure loaded, or no world lent (this is a read of the server's world: it answers during the server's hooks). |
| `watch` | `func(dimension: string, x: s32, y: s32, z: s32, id: string) -> result<option<u32>, string>` | Asks the game to follow the copy of structure `id` whose anchor is the mod's block entity at a position: about once a second the game checks it, calling `multiblock-events.formed` when it has come together and `broken` when it no longer holds. The answer is the quarter it is whole at — as of this call where the world is lent (the hook that calls it, a command), else as of the last check, about a second ago — and none where it isn't whole (or was never checked yet; kept across saves). Err where the mod has no block entity, or it watches over 256. Calling again is fine: it retells the same copy, and brings a check up to date at once where the world is lent. |

## `saved-data`

Access: capability `saved-data`.

Capability `saved-data`, server mods only (manifest `api = "0.3"`, experimental): bytes a mod keeps on the world itself — the members of an energy network, who owns a claim, a list of teleports — saved with the world in data/horizon/saved_data.json. Not the mod's `storage` folder: this travels with the world, so whoever opens it next, the mod reads back what it wrote. Read and written only during the server's hooks, as the block entities are.

| Function | Signature | What it does |
| --- | --- | --- |
| `get` | `func(key: string) -> option<list<u8>>` | The bytes at `key`, or none where the mod has none there. Keys are `[a-z0-9_.-]{1,64}`, one path segment, as `storage`'s. |
| `set` | `func(key: string, data: list<u8>) -> result<_, string>` | Replaces the bytes at `key`: an error for a malformed key, over 64 KiB at one key, over 256 keys or over 1 MiB for the whole mod. |
| `delete` | `func(key: string)` | Removes `key` and its bytes. No error where it has none. |
| `keys` | `func() -> list<string>` | The mod's keys, in order. |

## `menus`

Access: capability `menus`.

Capability `menus`, server mods only (manifest `api = "0.3"`, experimental): chest menus the mod opens for a player, with items of its own in the slots. Players see and click them but can't take the items; each click reaches `menu-events.clicked`. Any client shows them, vanilla's too, as they are vanilla's chest menus (`minecraft:generic_9x<rows>`).

### record `menu-item`

| Field | Type | |
| --- | --- | --- |
| `slot` | `u32` | From 0, left to right then top to bottom. |
| `item` | `string` | `minecraft:redstone`, ... |
| `count` | `u32` |  |
| `components` | `string` | Components beyond the item's own, in vanilla's item syntax: `[custom_name={text:"Energy",italic:false}]`; empty for none. |

### enum `slot-mode`

What a menu slot bound to an inventory takes.

| Case | |
| --- | --- |
| `storage` | Items go in and out, as in a chest. |
| `input` | Players only put items in (a machine's input, fuel). |
| `output` | Players only take items out (a machine's output). |

### record `menu-slot`

| Field | Type | |
| --- | --- | --- |
| `slot` | `u32` | The menu's slot. |
| `inventory-slot` | `u32` | The inventory's slot it shows. |
| `mode` | `slot-mode` |  |

| Function | Signature | What it does |
| --- | --- | --- |
| `open` | `func(player: string, menu: string, title: string, rows: u32, items: list<menu-item>) -> result<_, string>` | Opens a menu of `rows` rows (1 to 6) titled `title` for the player with this id, closing the menu they had open; `menu` is the mod's own name for it, given back with each click. Slots no item names are empty. An error for a player not on the server, rows out of range or a slot past the last, or for the window's own player in in-frame single player (not yet). |
| `set-items` | `func(player: string, menu: string, items: list<menu-item>) -> result<_, string>` | Replaces every slot of the player's menu `menu` of this mod, while it is open: an error where it isn't. |
| `close` | `func(player: string)` | Closes the player's menu of this mod, if one is open. |
| `open-inventory` | `func(player: string, menu: string, title: string, rows: u32, items: list<menu-item>, dimension: string, x: s32, y: s32, z: s32, slots: list<menu-slot>) -> result<_, string>` | Opens a menu as `open` does, whose slots `slots` show the inventory (capability `items`) of the mod's block entity at a position: players put their items in and take them out as in a chest, and shift-click moves them between it and their inventory. The menu follows the inventory as it changes. Its other slots are the mod's items, clicks on them reaching `menu-events.clicked`. |

## `displays`

Access: capability `displays`.

Capability `displays`, server mods only (manifest `api = "0.3"`, experimental): things a mod shows in the world that are not blocks: a block's model, an item's model or floating text, moved, scaled and turned freely, the client animating each change; moved without remaking it (`move`) and carried by one of the mod's mobs (`ride`). Each is a vanilla display entity (`block_display`, `item_display`, `text_display`) the server sends the players in its dimension; it lasts while the mod runs and is never saved.

### record `transform`

| Field | Type | |
| --- | --- | --- |
| `translation` | `tuple<f32, f32, f32>` | Moved by, in blocks. |
| `scale` | `tuple<f32, f32, f32>` |  |
| `rotation` | `tuple<f32, f32, f32, f32>` | Turned by, as a quaternion (x, y, z, w): (0, 0, 0, 1) not at all. |

### variant `shape`

| Case | Payload | |
| --- | --- | --- |
| `block` | `string` | A block's model (`minecraft:redstone_block`), from the display's position as its corner. |
| `text` | `string` | Floating text, turned toward whoever looks at it. |

### record `item-stack`

A stack whose model a display shows (`show-item`).

| Field | Type | |
| --- | --- | --- |
| `item` | `string` | `minecraft:nether_star`, `<mod>:<path>`. |
| `count` | `u32` | 1 to 99; a display's stack is a look, not storage. |
| `components` | `string` | Components beyond the item's own, in vanilla's item syntax; empty for none. Up to 512 bytes. |

| Function | Signature | What it does |
| --- | --- | --- |
| `show` | `func(name: string, dimension: string, x: f64, y: f64, z: f64, shape: shape, transform: transform, interpolation: u32) -> result<_, string>` | Shows the mod's display `name`, or changes it: its shape, position and transform, which the client moves to over `interpolation` ticks (0: at once). An error for an unknown dimension or block, or past 1024 displays. |
| `hide` | `func(name: string)` | Removes the mod's display `name`, if it has one. |
| `show-item` | `func(name: string, dimension: string, x: f64, y: f64, z: f64, stack: item-stack, transform: transform, interpolation: u32) -> result<_, string>` | Shows the mod's display `name` as `stack`'s model, or changes it, as `show` does its shapes: the client moves to the new position and transform over `interpolation` ticks. An error for an unknown dimension or item, a count or components out of range; past 1024 displays the game drops it (as `show`). |
| `move` | `func(name: string, x: f64, y: f64, z: f64, interpolation: u32) -> result<_, string>` | Moves the mod's display `name` — any shape — to a position: the client walks it there over `interpolation` ticks (0: at once; the client takes at most 59), the display is not remade. An error for a position that is not finite; a display the mod has none of does nothing. A display riding a mob is put down first. |
| `ride` | `func(name: string, entity: option<u32>) -> result<_, string>` | Makes the mod's display `name` ride the mod's mob `entity` (capability `entities`), of the same dimension, so it is carried along: it sits where the mob stands, lifted by its own transform. None puts it down where it stands. An error while mobs are not lent (the server's hooks only) or no mob of this mod has that id; past 16 displays on one mob the game drops the ride (as `show` past 1024). |

## `item-hooks`

Access: capability `item-hooks`.

Capability `item-hooks`, server mods only (manifest `api = "0.3"`, experimental): behaviour on the items the mod claims (`item-events.items`) — the use of the item itself, its blow on an entity, each tick it is held or worn — and bytes of the mod's own kept on a stack (energy in a tool, its mode), saved with the stack wherever it goes.

### record `held-stack`

| Field | Type | |
| --- | --- | --- |
| `item` | `string` | `minecraft:blaze_rod`, `<mod>:<path>`. |
| `count` | `u32` |  |
| `data` | `list<u8>` | The mod's own bytes on this stack, as `set-data` wrote them: empty for none. Saved with the stack, in its components. |

| Function | Signature | What it does |
| --- | --- | --- |
| `stack` | `func(player: string, slot: u32) -> option<held-stack>` | The stack in `slot` of `player`'s inventory (36 slots, hotbar first, then armour feet to head 36..39, then the off hand 40), when it is one of the mod's claimed items: none for an empty slot, another item or a player not on the server. Only during the server's hooks. |
| `set-data` | `func(player: string, slot: u32, data: option<list<u8>>) -> result<_, string>` | Replaces the mod's bytes on `player`'s stack in `slot` (none empties them). Err for a slot that isn't one of the mod's claimed items, a stack of more than one (a split copies a stack's components to both halves, so charge on a splittable stack would double: a stack that carries data keeps count 1), over 4 KiB, or more than 32 such writes in one call. Only during the server's hooks. |

## `item-events`

Access: exported by the mod: the game calls it.

What the server calls in a server mod with capability `item-hooks`. A server mod built before it has no such export and keeps working.

### type `player`

`type player = player`

### type `held-stack`

`type held-stack = held-stack`

### record `used-block`

| Field | Type | |
| --- | --- | --- |
| `dimension` | `string` | `minecraft:overworld`, ... |
| `x` | `s32` |  |
| `y` | `s32` |  |
| `z` | `s32` |  |
| `state` | `string` | The block, as `minecraft:oak_log[axis=y]`. |

### record `player-stack`

| Field | Type | |
| --- | --- | --- |
| `player` | `player` |  |
| `slot` | `u32` | 36 slots, hotbar first, then armour feet to head, then 40 the off hand; only the mod's claimed items are ever listed. |
| `stack` | `held-stack` |  |

### record `entity`

| Field | Type | |
| --- | --- | --- |
| `id` | `u32` | The entity's id, as the server counts entities. |
| `kind` | `string` | `minecraft:zombie`, ...; `minecraft:player` for a PvP blow. |
| `health` | `f32` | Its hit points before the blow. |

| Function | Signature | What it does |
| --- | --- | --- |
| `items` | `func() -> list<string>` | Once, after `init`: the items (`minecraft:stick`, a content mod's `mymod:wrench`) whose behaviour the mod takes, at most 256. None: the other hooks are never called. |
| `used` | `func(player: player, stack: held-stack)` | The player used (right-clicked) one of those items in the main hand, with nothing to use it on. The game does nothing else with the click (no eating, no throw, no swing-into-place), but a sneaking player keeps vanilla's use, and a block a mod takes the use of wins over the item (that mod hears `block_used`, this one nothing). |
| `used-on-block` | `func(player: player, stack: held-stack, block: used-block)` | The same click with the crosshair on a block, after the block's own vanilla use passed (a door opened still swallows the click). |
| `inventory-tick` | `func(tick: u64, held: list<player-stack>)` | Each server tick, after `block_entity_tick`: every stack of those items on every online player, held or worn. At most 512 a call: a longer list comes in turns — the game walks it round-robin from where the last tick stopped, so every stack is told within ceil(n / 512) ticks. Armour comes with its slot: 36 feet, 37 legs, 38 chest, 39 head. |
| `hurt-entity` | `func(player: player, stack: held-stack, entity: entity, damage: f32) -> result<f32, string>` | The player holding one of those items in the main hand struck `entity` (a mob, or a player in a PvP blow) for `damage` (vanilla's count, enchantments in): answer with what the blow should deal instead, 0 to cancel it. Only the number changes — knockback, sweep and the rest are vanilla's. An error cancels nothing (vanilla's damage). |
| `hurt` | `func(player: player, damage: f32, source: string) -> result<f32, string>` | `player` was hurt for `damage` (a damage type id, as `minecraft:generic`) while wearing one of those items: answer with how much of it the armour takes (0 to `damage`); what it takes is gone, the rest hurts as vanilla has it. The mod hears this once a blow whatever it answers. An error takes nothing. |

## `menu-events`

Access: exported by the mod: the game calls it.

What the server calls in a server mod with capability `menus`.

### type `player`

`type player = player`

| Function | Signature | What it does |
| --- | --- | --- |
| `clicked` | `func(player: player, menu: string, slot: u32, button: u8, shift: bool)` | The player clicked slot `slot` of the mod's open menu `menu` (only the menu's own slots, not their inventory's): `button` 0 left, 1 right; `shift` held. The menu shows the mod's items again after it. |
| `closed` | `func(player: player, menu: string)` | The player closed the mod's menu `menu` (or left the server). |

## `multiblock-events`

Access: exported by the mod: the game calls it.

What the server calls in a server mod with capability `multiblocks`. A server mod built before it has no such export and keeps working.

### record `multiblock`

A followed structure around one of the mod's block entities.

| Field | Type | |
| --- | --- | --- |
| `dimension` | `string` | `minecraft:overworld`, ... |
| `x` | `s32` |  |
| `y` | `s32` |  |
| `z` | `s32` |  |
| `state` | `string` | The anchor block, as `minecraft:oak_log[axis=y]`. |
| `data` | `list<u8>` | The mod's data on it (`block-entities`). |
| `structure` | `string` | The structure that came together or broke, as `cells:core`. |
| `quarter` | `u32` | The quarter turn it is whole at. |

| Function | Signature | What it does |
| --- | --- | --- |
| `formed` | `func(block: multiblock)` | The structure around the watched block entity came together (or was already whole when the mod asked to watch it: the first check after that reports it, within about a second). |
| `broken` | `func(block: multiblock)` | A structure that was whole no longer holds, or its block entity's block went. `data` is its last data, as `removed` gets. |

## `block-use-events`

Access: exported by the mod: the game calls it.

What the server calls in a server mod with capability `block-entities` that takes players' clicks on its blocks. A server mod built before it has no such export: its blocks keep vanilla's use.

### type `player`

`type player = player`

### record `used-block`

| Field | Type | |
| --- | --- | --- |
| `dimension` | `string` | `minecraft:overworld`, ... |
| `x` | `s32` |  |
| `y` | `s32` |  |
| `z` | `s32` |  |
| `state` | `string` | The block, as `minecraft:oak_log[axis=y]`. |
| `data` | `list<u8>` | The mod's data on it (`block-entities`). |

| Function | Signature | What it does |
| --- | --- | --- |
| `uses` | `func() -> list<string>` | Once, after `block-entity-events.blocks`: those of its claimed blocks whose use (a right click) the mod takes. A click on one does nothing else in the game (no vanilla use, no block placed against it), but a sneaking player with an item in hand places or uses it as vanilla does. |
| `used` | `func(player: player, block: used-block)` | The player used one of those blocks, with the main hand. |

## `block-entity-events`

Access: exported by the mod: the game calls it.

What the server calls in a server mod with capability `block-entities`. A server mod built before it has no such export and keeps working.

### record `block-entity`

| Field | Type | |
| --- | --- | --- |
| `dimension` | `string` | `minecraft:overworld`, ... |
| `x` | `s32` |  |
| `y` | `s32` |  |
| `z` | `s32` |  |
| `state` | `string` | The block there now, as `minecraft:oak_log[axis=y]`. |
| `data` | `list<u8>` | What the mod last stored on it: empty for a new one. |

| Function | Signature | What it does |
| --- | --- | --- |
| `blocks` | `func() -> list<string>` | Once, after `init`: the blocks (`minecraft:lodestone`, a content mod's `mymod:battery`) whose placed copies get the mod's block entities. None: the other hooks are never called. |
| `tick` | `func(tick: u64, entities: list<block-entity>)` | Each server tick: the mod's block entities in loaded chunks, after `tick`. |
| `removed` | `func(entity: block-entity)` | A block entity's block is gone (broken, replaced, blown up): its last data, with what stands there now. |

## `server-events`

Access: exported by the mod: the game calls it.

What the server calls in a server mod (manifest `api = "0.3"`, experimental). Only the hooks `init` asks for are ever called. The player in each event is as the server saw them when it happened.

### type `player`

`type player = player`

### record `block-pos`

| Field | Type | |
| --- | --- | --- |
| `x` | `s32` |  |
| `y` | `s32` |  |
| `z` | `s32` |  |

### variant `argument-kind`

What an argument of a command takes. The server sends each command and its arguments to players' games, whose chat box completes and colours them as vanilla's; the server parses them before the mod's `command` hook gets their values.

| Case | Payload | |
| --- | --- | --- |
| `word` |  | One word: letters, digits and `_ - . +`. |
| `text` |  | The rest of the line, spaces included: only last. |
| `integer` | `tuple<option<s32>, option<s32>>` | A whole number, within the bounds given. |
| `number` |  | A number with or without decimals. |
| `boolean` |  | `true` or `false`. |
| `player` |  | One online player: a name, or `@s` or `@p` (whoever typed it, as vanilla picks for a player). |
| `block-pos` |  | A block position: three whole numbers, each one also `~` or `~n`, relative to whoever typed it. |

### record `argument`

| Field | Type | |
| --- | --- | --- |
| `name` | `string` | Shown in the chat box's hint (`<height>`) and the key of its value. |
| `kind` | `argument-kind` |  |
| `optional` | `bool` | Optional arguments come after every required one. |

### record `server-command`

| Field | Type | |
| --- | --- | --- |
| `name` | `string` | Without the slash. A name vanilla already uses is refused (vanilla's command runs). |
| `arguments` | `list<argument>` | In order; none: the command takes nothing after its name. Free text is one optional `text` argument. |

### variant `argument-value`

An argument's value, parsed as its kind says.

| Case | Payload | |
| --- | --- | --- |
| `word` | `string` |  |
| `text` | `string` |  |
| `integer` | `s32` |  |
| `number` | `f64` |  |
| `boolean` | `bool` |  |
| `player` | `player` |  |
| `block-pos` | `block-pos` |  |

### record `registration`

| Field | Type | |
| --- | --- | --- |
| `commands` | `list<server-command>` | Commands players may type. |
| `tick` | `bool` | Call `tick` every server tick (20 per second). |
| `joins` | `bool` | Call `player-joined` and `player-left`. |
| `chat` | `bool` | Call `chat` for each chat message a player sends. |
| `blocks` | `bool` | Call `block-broken` and `block-placed`. |

| Function | Signature | What it does |
| --- | --- | --- |
| `init` | `func() -> result<registration, string>` | Once, after loading. An error keeps the mod off and is logged. |
| `tick` | `func(tick: u64)` | Each server tick, with the server's tick counter. |
| `player-joined` | `func(player: player)` | A player joined the server; they are in `players.online` already. |
| `player-left` | `func(player: player)` | A player left the server; they are no longer in `players.online`. |
| `chat` | `func(player: player, text: string)` | A chat message a player sent, as plain text. |
| `command` | `func(player: player, name: string, args: string, values: list<tuple<string, argument-value>>) -> result<option<string>, string>` | `/name args` typed by `player`: ok(some(text)) is their feedback, err(text) their error in red. `values` holds each argument given, by name, in order (an optional one left out isn't there); a line that doesn't parse never reaches the mod: the player gets vanilla's error. |
| `block-broken` | `func(player: player, pos: block-pos, state: string)` | A player broke the block at `pos` in their dimension: `state` is what was there, as `minecraft:oak_log[axis=y]`. |
| `block-placed` | `func(player: player, pos: block-pos, state: string)` | A player placed `state` at `pos` in their dimension. |

## `network`

Access: capability `network`.

Capability `network`, for server mods (manifest `api = "0.3"`) and client mods alike (experimental): bytes the two halves of one mod send each other. A mod's server half and its client half share the mod's id (two files, `side = "server"` and `side = "client"`): a message names the id, so it reaches the half with that id on the other side, and only that one. Channels are the mod's own names (`cell/state`), its bytes its own format. The client half reaches a Horizon server only: vanilla ones and vanilla clients are skipped, and a send to them is an error.

| Function | Signature | What it does |
| --- | --- | --- |
| `send-to` | `func(player: string, channel: string, data: list<u8>) -> result<_, string>` | Server mods: sends `data` on channel `channel` (`[a-z0-9_./-]{1,64}`, up to 30,000 bytes) to the player with this id. An error for a player not on the server or one that isn't a Horizon client, a channel or size outside the rules, or past 64 messages (256 KiB) in one call. Client mods get an error here. |
| `send` | `func(channel: string, data: list<u8>) -> result<_, string>` | Client mods: sends the same to the server half of the mod. An error when the server isn't a Horizon one that carries mod messages, or the same limits. Server mods get an error here. |

## `network-events`

Access: exported by the mod: the game calls it.

What the server calls in a server mod with capability `network`. A server mod built before it has no such export and keeps working.

### type `player`

`type player = player`

| Function | Signature | What it does |
| --- | --- | --- |
| `client-sent` | `func(player: player, channel: string, data: list<u8>)` | The client half of the mod sent `data` on `channel`: the player as the server saw them. A player sends at most 32 messages a tick; more are dropped. |

## `network-client-events`

Access: exported by the mod: the game calls it.

What the game calls in a client mod with capability `network`.

| Function | Signature | What it does |
| --- | --- | --- |
| `server-sent` | `func(channel: string, data: list<u8>)` | The server half of the mod sent `data` on `channel`. |

## `entities`

Access: capability `entities`.

Capability `entities`, server mods only (manifest `api = "0.3"`, experimental): mobs of the mod's own. Each is a vanilla mob (a zombie, an iron golem) the server runs as it runs any, with its hitbox, damage, knockback, loot and saving, marked as the mod's: the mod chooses its kind, switches its own AI off to steer it itself, hides its model to put display models over it (capability `displays`), and hears when it is struck, used and dies. The mark is kept with the mob in the world's save, so the mod finds its mobs again after a restart.

### record `entity`

| Field | Type | |
| --- | --- | --- |
| `id` | `u32` | The mob's entity id: stable while it lives. |
| `kind` | `string` | The vanilla mob it is: `minecraft:iron_golem`. |
| `tag` | `string` | The mod's own name for what it is (`sentinel`), set at spawn. |
| `dimension` | `string` |  |
| `x` | `f64` |  |
| `y` | `f64` |  |
| `z` | `f64` |  |
| `yaw` | `f32` |  |
| `health` | `f32` |  |
| `max-health` | `f32` |  |
| `dying` | `bool` | Its health is gone and the body waits to vanish (`died` told). |

### record `spawn-options`

| Field | Type | |
| --- | --- | --- |
| `kind` | `string` | A vanilla mob: `minecraft:zombie`, or just `zombie`. |
| `tag` | `string` | The mod's name for it, `[a-z0-9_./-]{1,64}`. |
| `no-ai` | `bool` | No goals and no movement of its own: the mod moves it. |
| `invisible` | `bool` | Its model is hidden: display models stand in. |
| `silent` | `bool` |  |
| `invulnerable` | `bool` |  |
| `no-gravity` | `bool` |  |
| `takes-use` | `bool` | A right click on it reaches the mod (`used`) and does nothing else. |
| `name` | `option<string>` | Its name, shown over it, up to 64 characters. |
| `health` | `option<f32>` | Its health at spawn (its kind's full health by default). |

| Function | Signature | What it does |
| --- | --- | --- |
| `spawn` | `func(dimension: string, x: f64, y: f64, z: f64, options: spawn-options) -> result<u32, string>` | Spawns a mob of the mod in `dimension` (the one the server is simulating: other dimensions are an error for now), at a position: its entity id, known at once. It exists from the end of the call. An error for an unknown kind, a bad name, a dimension, past 32 spawns a call or 1024 mobs of the mod, and where the game can't take it (event hooks that run mid-tick, a blow's `hurt`). |
| `get` | `func(id: u32) -> option<entity>` | One of the mod's mobs, as of the start of the call (or its spawn). |
| `list` | `func() -> list<entity>` | All of the mod's mobs, in the same way. |
| `teleport` | `func(id: u32, x: f64, y: f64, z: f64) -> result<_, string>` | Puts the mob at a position (a mob with its AI off moves only so). |
| `push` | `func(id: u32, x: f64, y: f64, z: f64) -> result<_, string>` | Adds a velocity to the mob (knockback, a leap). |
| `set-health` | `func(id: u32, health: f32) -> result<_, string>` | Sets its health, up to its full health: 0 kills it. |
| `remove` | `func(id: u32)` | Removes the mob without a death (no loot, no `died`; `removed` follows). |

## `entity-events`

Access: exported by the mod: the game calls it.

What the server calls in a server mod with capability `entities`. A server mod built before it has no such export and keeps working.

### type `player`

`type player = player`

### type `entity`

`type entity = entity`

| Function | Signature | What it does |
| --- | --- | --- |
| `tick` | `func(tick: u64, entities: list<entity>)` | Each server tick, after the block entities': the mod's mobs in the level, up to 512 (the first by id). |
| `hurt` | `func(player: player, entity: entity, damage: f32) -> result<f32, string>` | A player's blow with the hand or a weapon is about to hurt one of the mod's mobs for `damage`: answer with the damage to deal instead, 0 to cancel it (never more than four times). Only blows from players reach it; other damage lands as vanilla has it. An error cancels nothing. |
| `died` | `func(entity: entity)` | A mob of the mod died, whatever killed it: told once, as it starts to fall; its loot drops as vanilla's. |
| `removed` | `func(entity: entity)` | A mob of the mod is gone: after its death, removed, or despawned. Its last state. |
| `used` | `func(player: player, entity: entity)` | A player right-clicked a mob of the mod that takes the use. |

## `config`

Access: capability `config`.

Capability `config`, server mods only (manifest `api = "0.3"`, experimental): settings the mod declares once and the player edits in a file, `config/<mod id>.toml` beside the mod, while the game runs. The mod gives each entry a key, a description, a kind (a flag, a whole number or a number inside limits, text, one of some choices) and a default; the game writes the file with the descriptions and limits as comments, reads it again when it changes, and puts back the default for a value that is missing or wrong (and says why in the log). The mod reads values with `get` and hears `config-events.changed` when an edit changed some.

### record `integer-spec`

| Field | Type | |
| --- | --- | --- |
| `initial` | `s64` |  |
| `min` | `s64` |  |
| `max` | `s64` |  |

### record `number-spec`

| Field | Type | |
| --- | --- | --- |
| `initial` | `f64` |  |
| `min` | `f64` |  |
| `max` | `f64` |  |

### record `choice-spec`

| Field | Type | |
| --- | --- | --- |
| `initial` | `string` |  |
| `options` | `list<string>` | 1 to 32 options of 1 to 64 bytes: the value is one of them. |

### variant `spec`

What an entry holds, with its default and limits.

| Case | Payload | |
| --- | --- | --- |
| `flag` | `bool` | true or false, with its default. |
| `integer` | `integer-spec` |  |
| `number` | `number-spec` |  |
| `text` | `string` | Text up to 1024 bytes, with its default. |
| `choice` | `choice-spec` |  |

### record `entry`

| Field | Type | |
| --- | --- | --- |
| `key` | `string` | `[a-z0-9_.-]{1,64}`, the key in the file. |
| `description` | `string` | Up to 512 bytes, written above the entry in the file. |
| `spec` | `spec` |  |

### variant `value`

A value: a choice is text.

| Case | Payload | |
| --- | --- | --- |
| `flag` | `bool` |  |
| `integer` | `s64` |  |
| `number` | `f64` |  |
| `text` | `string` |  |

| Function | Signature | What it does |
| --- | --- | --- |
| `declare` | `func(entries: list<entry>) -> result<_, string>` | Declares the mod's entries (up to 128; a second call replaces them) and reads the file: the file is written with every entry when it is missing or lacks one. An error for a bad key, a default outside its limits, an empty choice or a key twice. Call it in `init`. |
| `get` | `func(key: string) -> option<value>` | The value of an entry, as the file holds it now: none for a key the mod did not declare. |
| `set` | `func(key: string, value: value) -> result<_, string>` | Sets an entry and writes the file, for a command or a screen of the mod's own. An error for an undeclared key, another kind of value, or a value outside the limits. The mod is not told of its own change. |

## `config-events`

Access: exported by the mod: the game calls it.

What the server calls in a server mod with capability `config`. A server mod built before it has no such export and keeps working.

| Function | Signature | What it does |
| --- | --- | --- |
| `changed` | `func(keys: list<string>)` | The file was edited and these entries have new values (checked: a wrong value is already its default again). The game looks every five seconds. |

## `inter-mod`

Access: capability `inter-mod`.

Capability `inter-mod`, server mods only (manifest `api = "0.3"`, experimental): messages between server mods — a library mod's code shared by the mods that need it. A mod's manifest lists what it provides (`provides = ["spin-lib@1.0"]`) and what it needs (`requires = ["spin-lib@1"]`); the game starts providers before the mods that need them and keeps a mod off, with the reason, whose need no turned-on mod provides. Messages are bytes on a channel, the mods' own names and format (as `network`'s), delivered at the end of the server's tick, in rounds, so an answer arrives within the same tick.

### record `provider`

A mod that provides an API, as `expose` registered it.

| Field | Type | |
| --- | --- | --- |
| `mod-id` | `string` | The providing mod's id. |
| `version` | `string` |  |

| Function | Signature | What it does |
| --- | --- | --- |
| `expose` | `func(name: string, version: string) -> result<_, string>` | Registers this mod as serving API `name` at `version`, for `providers`. `name` `[a-z0-9_.-]{1,64}` and `version` `major[.minor[.patch]]` must match one `provides` entry of the manifest. At most 8 a mod. |
| `providers` | `func(name: string) -> list<provider>` | The running mods serving API `name` — their ids and versions — as of this call. A library turned off is not here. |
| `subscribe` | `func(channel: string) -> result<_, string>` | Receives `publish` of `channel`: `[a-z0-9_./-]{1,64}`, at most 16 a mod. Messages wait until the tick's end. |
| `publish` | `func(channel: string, data: list<u8>) -> result<_, string>` | Sends `data` (up to 30,000 bytes) on `channel` to every other mod that subscribed it (a subscriber that cannot hear messages — no `inter-mod-events` export — is skipped). Err for a bad channel or size, or past 64 messages (256 KiB) in one call. |
| `send-to` | `func(mod-id: string, channel: string, data: list<u8>) -> result<_, string>` | Sends to one running server mod, however it gets the channel: the other mod hears it in `inter-mod-events.message`. Err for a mod that is not running or takes no messages (it runs without the `inter-mod-events` export), the mod's own id, a bad channel or size, or the same per-call limits. |

## `inter-mod-events`

Access: exported by the mod: the game calls it.

What the server calls in a server mod with capability `inter-mod`. A server mod built before it has no such export and keeps working.

| Function | Signature | What it does |
| --- | --- | --- |
| `message` | `func(from: string, channel: string, data: list<u8>)` | Another mod sent `data` on `channel` (`publish` of a channel this mod subscribed, or `send-to` to it); the sender's id. At most 64 messages and 512 KiB reach a mod in one tick; the rest wait for the next one. |

## `game-events`

Access: exported by the mod: the game calls it.

Capability `game-events`, server mods only (manifest `api = "0.3"`, experimental): the server tells the mod what happens to players and the world, as a mod bus does: a player hurt, dead, back, in another dimension or game mode, a mob a player killed, the weather turning. These are notices, after the fact (a mod that must change the outcome uses the hooks of the capability that owns it: `item-hooks`' `player-hurt`, `entities`' `hurt`). A server mod built before it has no such export and keeps working. Within one tick the order of different events is not promised.

### type `player`

`type player = player`

### type `game-mode`

`type game-mode = game-mode`

| Function | Signature | What it does |
| --- | --- | --- |
| `player-hurt` | `func(player: player, amount: f32, health: f32)` | A player's health fell (`amount` points, to `health`): any damage, whatever its source. Not told for a player who died of it (`died` follows); a player healing is not told. |
| `player-died` | `func(player: player, cause: string)` | A player died. `cause` is the death message as vanilla writes it ("Alex fell from a high place", "Alex was slain by Zombie"); empty when the `showDeathMessages` rule hides it. |
| `player-respawned` | `func(player: player)` | A dead player is alive again (they respawned). |
| `dimension-changed` | `func(player: player, before: string, after: string)` | A player is in another dimension than the last tick. |
| `game-mode-changed` | `func(player: player, before: game-mode, after: game-mode)` | A player's game mode changed. |
| `mob-killed` | `func(player: player, kind: string, id: u32)` | A player's blow killed a mob: its kind (`minecraft:zombie`) and its entity id. Melee blows only: a bow, a trident or an explosion are not told. |
| `weather-changed` | `func(weather: string)` | The weather turned: `clear`, `rain` or `thunder`. |

## `effects`

Access: capability `effects`.

Capability `effects`, server mods only (manifest `api = "0.3"`, experimental): particles and sounds in the world the server simulates, sent to the players near them (particles within 32 blocks, a sound as far as its volume carries: 16 blocks a unit of volume, at least 16). Players on a Horizon client or a vanilla one both get them. Names are the game's: `minecraft:flame`, `minecraft:block.anvil.land`; a sound the game does not know is sent by name and played when the player's resource pack has it. A mod sends at most 128 effects a call.

### record `particle`

| Field | Type | |
| --- | --- | --- |
| `name` | `string` | `minecraft:flame`, `minecraft:happy_villager`, `minecraft:dust`: any particle the game has. One that needs more than a name takes it from the fields below: `color` and `scale` for `dust`, `entity_effect`, `effect`, `instant_effect` and `tinted_leaves`; `block` for `block`, `block_marker`, `falling_dust`, `dust_pillar` and `block_crumble`. Items, trails and vibrations are not sent. |
| `x` | `f64` |  |
| `y` | `f64` |  |
| `z` | `f64` |  |
| `spread-x` | `f32` | How far from the point they scatter on each axis (0 to 64), and the speed they move at (0 to 10): `count` of them, each with a gaussian spread. With a `count` of 0 one particle is sent, and it moves by speed times spread. |
| `spread-y` | `f32` |  |
| `spread-z` | `f32` |  |
| `speed` | `f32` |  |
| `count` | `u32` | How many, up to 500. |
| `color` | `option<u32>` | 0xRRGGBB. |
| `scale` | `option<f32>` | A dust's size, 0.01 to 4 (1 by default). |
| `block` | `option<string>` | A block state, `minecraft:stone` or `minecraft:oak_log[axis=y]`. |
| `always-show` | `bool` | Drawn from any distance and whatever the player's particle setting says. |

| Function | Signature | What it does |
| --- | --- | --- |
| `spawn-particle` | `func(dimension: string, particle: particle) -> result<_, string>` | Sends a particle in `dimension` (the one the server is simulating: other dimensions are dropped). An error for a name that is not `namespace:path`, values out of range, or past 128 effects a call; a particle the game does not know, or one missing the fields it needs, is dropped and logged. |
| `play-sound` | `func(dimension: string, sound: string, x: f64, y: f64, z: f64, volume: f32, pitch: f32, category: string) -> result<_, string>` | Plays a sound at a position in `dimension`. `volume` is 0 to 10 (carrying 16 blocks a unit), `pitch` 0.5 to 2, `category` one of `master`, `music`, `record`, `weather`, `block`, `hostile`, `neutral`, `player`, `ambient`, `voice`: the volume slider the player hears it on. An error for a bad name or category, or past 128 effects a call. |
| `play-sound-to` | `func(player: string, sound: string, volume: f32, pitch: f32, category: string) -> result<_, string>` | Plays a sound for one player, at their position (a cue, not a place in the world). Nothing for a player who is not on the server. |

## `redstone`

Access: capability `redstone`.

Capability `redstone`, server mods only (manifest `api = "0.3"`, experimental): redstone for the mod's own block entities. The game reads the signals on each face of every block the mod claims, every tick, and tells the mod when they change; and the mod sets the signal each face gives out, which vanilla blocks around it (wire, lamps, repeaters, doors, pistons) read as they read a redstone block or a lever. Signals are 0 to 15. Faces are named as the block's own: `north` is the face towards -Z, `up` the top.

### record `sides`

A signal on each of a block's six faces.

| Field | Type | |
| --- | --- | --- |
| `down` | `u8` |  |
| `up` | `u8` |  |
| `north` | `u8` |  |
| `south` | `u8` |  |
| `west` | `u8` |  |
| `east` | `u8` |  |

### record `output`

What a block gives out.

| Field | Type | |
| --- | --- | --- |
| `power` | `sides` | The strength on each face: what the block touching that face reads. |
| `strong` | `bool` | Also powers the blocks it touches through, as a lever powers the block it is on (a redstone block does not). |

| Function | Signature | What it does |
| --- | --- | --- |
| `get-input` | `func(dimension: string, x: s32, y: s32, z: s32) -> option<sides>` | What the mod's own block entity at a position reads from the blocks around it: on each face, the signal the neighbour there gives it (wire, a lever, a redstone block, another mod's block), as of the start of the tick. None for a block that is not the mod's, or not read yet (a block entity is read from the tick after it appears). |
| `set-output` | `func(dimension: string, x: s32, y: s32, z: s32, output: output) -> result<_, string>` | Sets what the mod's own block entity at a position gives out; all zeros stops it. The blocks around it hear at once (they update as they do for a lever). An error where the mod has no block entity, for a strength over 15, or where another mod drives the signal. The signal is saved with the world and goes with its block entity. |
| `get-output` | `func(dimension: string, x: s32, y: s32, z: s32) -> option<output>` | What the block at a position gives out, whichever mod drives it: none where it gives nothing. |

## `redstone-events`

Access: exported by the mod: the game calls it.

What the server calls in a server mod with capability `redstone`. A server mod built before it has no such export and keeps working.

### type `sides`

`type sides = sides`

| Function | Signature | What it does |
| --- | --- | --- |
| `input-changed` | `func(dimension: string, x: s32, y: s32, z: s32, input: sides)` | What one of the mod's block entities reads from its faces changed, or was read for the first time (after it was placed, or the world or the mod started), or a neighbouring block changed: at most 256 a tick, the rest the tick after. |
