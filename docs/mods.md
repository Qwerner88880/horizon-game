# Horizon mods

Horizon gets its own mod system, written in Rust and run in a sandbox, plus
two ways to bring over what Java players use: native Horizon ports of popular
mods and the data-driven parts of packs and mods. Fabric and NeoForge jars do
**not** run as they are: their code calls and patches Minecraft's Java classes, and
Horizon has none. What carries over is the data and assets inside a jar
([modpacks.md](modpacks.md)).

Mods are **off and absent by default**. A fresh install has no `horizon-mods`
folder, loads no runtime and plays exactly like vanilla 26.3.

| Part | Where | State |
|---|---|---|
| The contract (WIT) | `crates/horizon-mod-sdk/wit/horizon.wit` | 0.1: log, game-read, chat, hud, storage, command, ui; tick, frame, chat and command hooks |
| SDK for mod authors | `crates/horizon-mod-sdk` | 0.1 |
| Example mods | `crates/horizon-mod-sdk/examples/coords-hud`, `examples/predator-missile` | coordinates, facing and time HUD, `/where`, `/coords`; `/missile`, a guided missile from vanilla commands |
| Horizon's official mods | `mods/official/` | the extras moving to the registry: FPS Counter (`fps-overlay`), Keystrokes, Zoom, Minimap; Far View (`far-view`), distant terrain; and Recipes, a recipe viewer |
| Runtime | `crates/horizon-mods` | wasmtime 48 LTS, component model |
| Authoring tool | `crates/cargo-horizon-mod` | `cargo horizon-mod new [--template code\|content] / build / run / test / check`, and the registry's CI (`registry build / review / list`) |
| In the game | `crates/horizon-client/src/game/mods.rs`, Options > Mods | client mods |
| Launcher mod browser (Modrinth, CurseForge with a key) | [mod-browser.md](mod-browser.md) | resource packs, modpacks |
| Registry index (`index.json`, `.hpack`), signatures | `horizon_mods::index`, [api/mod-index.md](api/mod-index.md) | schema 1; launcher and website next |
| Server mods, registries | this page, [Roadmap](#roadmap) | planned |

## Design goals

1. **Safe to install.** A mod is untrusted code. It runs in a WebAssembly
   sandbox with no file, network, process or clock access, a memory cap and a
   time budget per call. It can only do what its capabilities grant, and the
   player sees those before turning it on.
2. **One file for every platform.** The same `.hmod` runs on Windows, macOS
   and Linux, x86-64 and ARM: authors build once, players never meet "wrong
   build for my OS".
3. **Free when unused.** No mod on: no engine, no thread, no compile, one
   branch per hook. Measured below.
4. **Vanilla stays vanilla.** Mods add; they cannot patch the game's code the
   way Mixins do. What they may change is a list of hooks the game calls at
   fixed points, so the parity harnesses keep testing the same game.
5. **Stable for authors.** The contract is a versioned WIT package; a mod
   built for API 0.1 keeps loading on later 0.1.x games.

## How it works

```
 mod crate (Rust)                         Horizon
 ───────────────                          ───────
 impl Mod for MyMod { ... }               horizon-mods::Mods
 export_mod!(MyMod)                         ├─ Engine (cranelift), made on first mod
        │ cargo horizon-mod build           ├─ one Store per mod: 64 MiB, fuel per call
        ▼                                   ├─ Linker: log, game, chat, hud (WIT imports)
 my_mod.hmod = zip {                        └─ calls the events export: init, tick,
   horizon-mod.toml,                             frame, command, chat
   mod.wasm  (a WebAssembly component)
 }
```

- **WebAssembly components.** Mods compile to `wasm32-unknown-unknown`;
  `cargo horizon-mod build` wraps the module into a
  [component](https://component-model.bytecodealliance.org/) for the
  `horizon:mods/horizon-mod` world. The WIT file is the whole API: host
  functions the mod imports (`log`, `game`, `chat`, `hud`) and the `events`
  interface it exports. No WASI: the component cannot even name a file or a
  socket.
- **Capabilities are imports.** Every imported interface except `log` is a
  capability. The manifest declares them; the game reads the component's
  actual imports and refuses a mod whose code imports one its manifest does not
  declare (`uses capability chat, missing from horizon-mod.toml`). So the list
  the player reads is the truth, not a promise.
- **Hooks on request.** `init` returns a `registration`: the commands the mod
  owns and which of `tick`, `frame` and `chat` it wants. The game calls only
  those. A mod that wants nothing per frame costs nothing per frame.
- **Retained HUD.** A mod sets its panel (corner and coloured lines) when it
  changes; the game draws it every frame with the extras' overlay code. Most
  HUD mods need no frame hook at all.
- **Snapshots, not callbacks into the world.** Once per tick (and before a
  command) the game copies what mods may read (player position, look, health,
  food, dimension, world clock) into a snapshot shared by all mods. Mods never
  hold references into the game, so they cannot stall or corrupt it.

### What a mod can do (API 0.1)

| Interface | Capability | Functions |
|---|---|---|
| `log` | always | `write(level, message)`, in the game log as `[mod <id>]` |
| `game` | `game-read` | `get-player()`: x y z, yaw, pitch, health, food, dimension; `get-clock()`: game time, day time; `raycast(distance)`: the first block along the view (fluids pass), up to 1024 blocks and across the distant terrain, with the point hit and the block's id |
| `chat` | `chat` | `show(text)`: a line only the local player sees, after the mod's name (8 per call) |
| `hud` | `hud` | `set-panel(corner, lines)`, `clear()`: up to 16 lines of 256 characters |
| `storage` | `storage` | `get(key)`, `set(key, value)`, `delete(key)`: a per-mod folder of up to 64 files, 4 MiB total, at most 1 MiB written per call |
| `command` | `command` | `run(line)`: a vanilla command run as the player, with the player's permissions, after the hook returns (8 per call); mods' own commands are not run. `run-quiet(lines)`: up to 256 a call, the same rights, nothing in the chat (single player and Horizon's servers run them 64 a tick and log one line a batch; elsewhere they go as chat commands and their feedback shows) |
| `ui` | `ui` | `open-screen`, `close-screen`, `set-settings`: a screen the mod owns and a page in Options > Mods, of text, button, item and list widgets the game lays out and draws with its own menus' code; `browser.set-items`, `browser.poll`: a recipe viewer's panel of items right of the inventory and container screens (not the creative one), which the game pages and draws with its own slots and search box, and the mod hears its clicks, its searches and the mod's own keys (`input.register-key`) pressed over any item on the screen; `browser.set-recipes`: recipes in the panel's place (crafting grids, cooking, stonecutting, smithing and brewing layouts, their slots cycling through their stacks, a note and an action button each), paged by the game, with clicks on their slots, action buttons, page turns and a back button. **Experimental:** the manifest needs `experimental = ["ui"]` |
| `render` | `render` | `set-hud(shapes)`: up to 1024 fills, four-cornered shapes, GUI sprites, items and text, kept on the HUD (over the hotbar, under chat and screens, hidden with F1) until replaced; `set-corner(corner, w, h, shapes)`: a box the game places in a corner, stacked with its overlays and other panels, as `hud`'s panels are; `text-width(text)`; `frame-stats()`: the game's FPS, min and 1% low; `gui-size()`; `shake(strength, seconds)`: the view turns, the player doesn't, scaled by Distortion Effects; `set-zoom(zoom)`: the field of view divided by up to 10, eased or at once, with slower turning, while playing and not through a spyglass (the strongest of several mods' zooms counts); `set-minimap(minimap)`: a map of the world around the player in a corner, as a map item colours it (the floor underground and under roofs), square or round, turning with the player or north up, with the player's arrow; the game keeps its tiles within its own frame budget (one map at a time, the first loaded mod's); `set-far-terrain(far-terrain)`: the world past the render distance as distant terrain, out to 64 to 1024 chunks, built by the game's own workers (the player's LOD Distance, when on, wins; the first loaded mod's). The game's own HUD renderer draws them: a mod never supplies shader code. **Experimental:** `experimental = ["render"]` |
| `input` | `input` | `claim-items(ids)`: the use button on those items is the mod's (`use-start`, `use-stop` with the ticks held; the game doesn't place, eat or swing); `register-key(name, label, key)`: up to 16 keys (`key.keyboard.r`), heard while playing as `key-down` / `key-up` (the game's own mapping on that key still runs; a screen opening lets held keys up), mouse buttons as `key.mouse.4`. The player rebinds them on the mod's page in Options > Mods, where the game lists them first as Key Binds rows (click, then press a key or mouse button; Esc binds none), even for a mod without `ui`; the choice is kept in `horizon-mods.txt` (`key.<mod>.<name>:key.keyboard.z`) and wins over `register-key`'s default; `claim-wheel(on)`: the wheel's steps become `wheel` events and the hotbar doesn't move (the one input a mod takes from the game; a mod's keys never do); `held()`; `mapping(name)`: a movement or click mapping's key label and whether it is held; `clicks-per-second()`; `poll()` returns the events, at most 256 kept. **Experimental:** `experimental = ["input"]` |
| `recipes` | `recipes` | `items()`: every item in the registry's order, with its name in the player's language and its tags; `all()`: every recipe (crafting, smelting, blasting, smoking, campfire, stonecutting, smithing, brewing), its slots as the stacks that fit each, its results, cooking time and experience, and the recipe book's id for it; `known()`: the book ids the player's recipe book holds; `place(book-id, all)`: the recipe book's click on a recipe, in the open inventory's or crafting table's grid (refused for a recipe the player's book doesn't hold, as vanilla's server refuses it); `give(stack)`: in creative mode, the stack into the player's inventory, as from the creative inventory. The recipes are the world's in single player and on Horizon's servers, vanilla 26.3's on any other server (a client is sent no others), while `known` follows that server's book. The game makes the tables once, when a mod first reads them. **Experimental:** `experimental = ["recipes"]` |

| Hook (export) | When |
|---|---|
| `init() -> result<registration, string>` | once, after loading; an error keeps the mod off and is shown |
| `tick(tick)` | each game tick while in a world (20 per second) |
| `frame(seconds)` | each rendered frame |
| `command(name, args) -> result<option<string>, string>` | `/name args` typed in chat, for a command the mod registered |
| `chat(text)` | each chat line the player receives, as plain text |

Commands: vanilla's command names win. A mod asking for `/give` does not get
it (logged), and two mods cannot share a name (the first one loaded keeps it).
The chat box colours and Tab-completes a mod's commands like vanilla's, for
every player whatever their permission level, and takes any text after the
name (the mod reads it as `args`; API 0.1 has no way to declare argument types).

### Planned interfaces

Each is a new capability, added in a 0.x minor version so older mods keep
loading. Ordered by what the first native ports need:

| Interface | For | Notes |
|---|---|---|
| `keys` | keybinds | registered binds appear in Controls; press/release events |
| `target` | the crosshair's block or entity, with its state and NBT summary | Jade/WTHIT-style tooltips |
| `inventory-read` | the player's inventory, held item, open container | AppleSkin, recipe viewers |
| `world-read` | blocks and biomes around the player, within the render distance | minimaps, schematics |
| `render` (world) | world-space lines, boxes and billboards, beside the HUD shapes `render` has | Litematica-style placement, waypoints |
| `registries` | new blocks, items, entities and recipes, mostly data: JSON models, textures, loot and recipes in the `.hmod`'s `assets/` and `data/` | server side; see [Multiplayer](#multiplayer) |
| `server` | world events (entity spawn/death), game rules, world read/write, typed server commands | after [server mods 0.3](#server-mods-api-03-experimental)'s players, chat, blocks and commands |
| `net` | a mod's own client↔server channel (custom payloads) | only between the two halves of the same mod |

There is no file, socket, process or raw-input interface, on purpose.

## Choosing the runtime

| Option | Sandbox | One file for all OSes | Typed API | Cost | Verdict |
|---|---|---|---|---|---|
| **wasmtime + component model** | yes: memory, fuel, no imports but ours | yes | WIT → Rust on both sides | 85 ns per empty hook call; 50 ms to compile a 64 KB mod on load | **chosen** |
| wasmtime, core modules with a hand ABI | yes | yes | no: pointers and lengths by hand | a few ns less per call | not worth the fragile ABI |
| wasmi (interpreter) | yes | yes | no components | no compile time, execution ~10-50x slower than cranelift code | kept in mind for a platform without JIT |
| Lua (mlua) or another script | partial (strip the stdlib, hope) | yes | no | fast to write | not Rust; weak sandbox |
| native `cdylib` plugins | none | no: per OS and arch, unstable Rust ABI | yes | none | unsafe for players |
| an embedded JVM | none per mod | yes | Java | 100+ MB, seconds of startup | see the research |

wasmtime 48 is a long-term-support line (security fixes until August 2028)
and needs Rust 1.95. It reads WebAssembly with the same wasm-tools release
(0.254) as the `wit-component` that packs mods and the SDK's `wit-bindgen`, so
the build carries one copy of them. Only its `runtime`, `cranelift`,
`component-model` and `std` features are on: no WASI, no async, no GC
proposal, no profiling. Its cranelift backend generates native code for x86-64
and ARM64 on Windows, macOS and Linux, which is why one `.hmod` serves them
all.

The game links it only with its `mods` feature: every release build turns it
on (`release.yml`), and a build from source needs
`cargo build --release -p horizon-client --features mods` to run code mods.
Without it (the tests and probes) wasmtime and cranelift aren't built, a
quarter of a clean build; content mods still load, and the game logs why the
code mods don't start.

### Performance

Measured on the Mac mini (M4 Pro), release build, `cargo test -p
horizon-mods --features runtime,pack --release --test sandbox -- --ignored --nocapture`:

| | per call |
|---|---|
| tick with no mod loaded | 1.2 ns (one check) |
| empty `tick` hook (call into the sandbox and back, fuel on) | 85.5 ns |
| empty `frame` hook | 87.5 ns |
| coords-hud's `tick` (reads the player and clock, formats 3 lines, compares) | 2.3 µs |
| loading coords-hud (read, compile with cranelift, instantiate, `init`) | 50 ms, once |

In the game, same world and view, `horizon-ctl bench 10` three times each
(4 other sessions shared the machine, so the tails are noisy; the median is
the stable figure):

| | median frame | mean FPS |
|---|---|---|
| no `horizon-mods` folder | 2.62-2.71 ms | 149-163 |
| coords-hud on | 2.72-2.78 ms | 151-254 |

At 20 ticks per second coords-hud costs 46 µs per second of game time: well
under 0.01 ms per frame. The frame-time medians differ by less than the
run-to-run spread.

**Budgets.** Each call gets fuel (about one unit per WebAssembly instruction):
2 billion for `init`, 50 million for `tick`, 10 million for `frame`, 100
million for a command or chat line. A runaway loop ends in a fraction of a
second (the sandbox test's endless `/spin` is cut well within its 5 s limit)
instead of hanging the game. The target budgets a mod should stay under are
tighter: **1 ms per tick and 0.1 ms per frame**. The runtime keeps each mod's
average call time (`Entry::cost`); the Mods page will show it, and a mod over
budget for long will be flagged, then paused.

### Crash isolation

A trap (panic, out-of-bounds access, `unreachable`), running out of fuel or
hitting the 64 MiB memory cap stops **that mod only**. The game says so in
chat (`Mod Test Guest stopped: it took too long (an endless loop?)`), marks it
on the Mods page and carries on; the other mods keep running. Host functions
clip what a mod hands back (chat lines per call, HUD lines and lengths). Tests:
`crates/horizon-mods/tests/sandbox.rs` (an endless loop, a panic and a 1 GiB
allocation, next to a healthy mod; `cargo test -p horizon-mods --features
runtime,pack`).

## Playing with mods

1. Put `.hmod` files (or mod folders holding `horizon-mod.toml` and
   `mod.wasm`) in `horizon-mods/` beside `options.txt` (in the launcher's game
   folder).
2. Options > **Mods** lists them with what each may do ("May see your
   position, health and the time, draw text on your screen, ...") and a
   switch. They start off; the choice is kept in `horizon-mods.txt`
   (`mod.<id>:true`).
3. A mod that failed to load or stopped shows why, in red, on its row.

Two turned-on mods never share an `id` while running (it is also capability
`storage`'s directory): the one `find` lists first starts, the other stays
off with the reason on its row.

Benchmarks (`--benchmark`) never load mods.

## Making a mod

```sh
rustup target add wasm32-unknown-unknown
cargo install --git https://github.com/horizon-rust/horizon cargo-horizon-mod
cargo horizon-mod new my-mod        # Cargo.toml, horizon-mod.toml, src/lib.rs, tests/hello.toml
                                    # (--template content: a content mod, no code)
cd my-mod
cargo horizon-mod run               # build, then start the game with it
cargo horizon-mod build             # again after an edit: the running game reloads it
cargo horizon-mod test              # build, then run tests/*.toml with no game
```

`run` starts `horizon-client` (on `PATH`, or `--game PATH`, or
`HORIZON_CLIENT`) with `--dev-mod target/horizon-mod/<id>.hmod`. A dev mod is
always on, and the game checks its file once a second: rebuild and it restarts
with "Mods reloaded" in chat. Its in-memory state starts over; whatever it
saved with capability `storage` survives the reload.

Before you submit it to the registry, run what its CI runs:

```sh
cargo horizon-mod check target/horizon-mod/my_mod.hmod   # --json for the report as JSON
```

It reads the manifest and content, refuses imports the capabilities don't
cover, then plays the mod in the sandbox for 40 ticks, 120 frames, a chat line
and each of its commands: it must start and never trap or overrun. It checks a
`.hpack` and a resource pack `.zip` too, and a signature with `--key <public
key> --signature <file>.minisig`.

### Testing a mod

`cargo horizon-mod test` builds the mod, then runs each `tests/<name>.toml` in
the sandbox the game uses, with no game and no window: the test plays the
game's part (the player and the clock the mod reads, ticks, frames, chat lines,
commands; for a server mod, players joining, chatting, typing commands,
breaking and placing blocks and leaving) and checks what the mod showed. A script fails at its first wrong
step, with what it expected and what it got; the command fails if any script
does. `--debug` builds without optimizing, `--hmod <file>` tests a `.hmod`
already built, and a name runs only the scripts whose path holds it. Each
script gets a scratch copy of the mod, so its `storage` starts empty and is
removed afterwards.

```toml
# tests/panel.toml (examples/coords-hud)
[player]                 # where the script starts; unset fields keep the defaults
x = 12.5                 # (0.5 64 0.5, yaw 0, 20 health and food, the Overworld)
z = -3.25
yaw = 180.0

[clock]                  # day-time 0 is 6:00 on day 1; each tick adds one
day-time = 29999

[[step]]
ticks = 1
expect.panel = ["XYZ 12.5 / 64.0 / -3.2", "Facing North (-Z)", "Day 2, 12:00"]
expect.corner = "top-left"

[[step]]
command = "coords"
expect.feedback = "Coordinates panel hidden"
```

A step's parts happen in this order: `reload`, `title-screen`, `blocks`,
`join`, `player`, `clock`, `hold`, `ticks`, `frames`, `chat`, then — for
a server mod — `use`, `use-item`, `attack`, `hurt`, `strike` and `interact` (`strike = {
tag = "sentinel", damage = 6.0 }` has the player hit the mod's mob of that
tag, `interact = { tag = "sentinel" }` right-click it; `expect.dealt` is the
damage its mod let through; the mobs of a test have 20 health, ids from 1000,
and may be a zombie, skeleton, creeper, iron golem, cow, pig, sheep or
villager) (`use-item = { item =
"minecraft:blaze_rod", on = { x = 0, y = 63, z = 0, state =
"minecraft:lodestone" } }` uses the main hand's stack in the air or on a
block, `attack = { item = ..., kind = "minecraft:zombie", health = 20.0,
damage = 4.0 }` strikes an entity with it, `hurt = { amount = 10.0,
source = "minecraft:fireworks" }` hurts the player), then `click`,
`command`, `break`, `place` and `leave`.

| Key | Does |
| --- | --- |
| `reload = true` | stops the mod and starts it again, as a rebuild does in the game: its memory starts over, its `storage` stays |
| `title-screen = true` | no player and no clock until a step sets them |
| `blocks = [{ x = 0, y = 63, z = 0, state = "minecraft:stone" }]` | server mods with `world`: those blocks of the Overworld, before the rest of the step (every other block is air, every biome the dimension's plainest: plains, nether wastes, the end) |
| `player = { x = 3.0, ... }`, `clock = { day-time = 0 }` | changes those fields (`x y z yaw pitch health food dimension`, `game-time day-time`) |
| `hold = { item = "minecraft:blaze_rod", count = 1, slot = 0 }` | server mods with `item-hooks`: the player holds that stack, fresh with no data of a mod's on it (count and slot optional; slot 0 is the main hand, armour feet to head 36..39, the off hand 40) |
| `ticks = n`, `frames = n` | that many ticks (the game's tick counter and the clock move on), then frames 1/60 s apart |
| `chat = "<Steve> hi"` | a chat line the player receives |
| `command = "where north"` | the player types `/where north`; a command the mod doesn't own fails the step |
| `break = { x = 1, y = 63, z = 0, state = "minecraft:stone" }`, `place = { ... }` | server mods: the player breaks or places that block |
| `leave = "Alex"` | server mods: that player leaves |

A server mod starts with nobody on: `join = "Alex"` (after `reload` and `title-screen`) brings a
player in at the defaults, in survival, with the id
`00000000-0000-4000-8000-000000000001` for the first name that joined, `...2`
for the second, the same id when a name joins again. `player`, `chat`,
`command`, `break` and `place` are then that player's doing: the one named by
`as = "Steve"`, or the first one on. `player` takes `game-mode` there, and the
script's own `[player]` and `[clock]` are a client mod's.

Then `expect` checks what the mod did during the step; a key left out is not
checked:

| `expect.` | Checks |
| --- | --- |
| `commands = ["where"]` | the commands the mod owns |
| `chat = ["..."]` | every chat line it showed, in order (`[]`: none) |
| `feedback = "..."`, `error = "..."` | the command's answer, shown plain or in red |
| `panel = ["..."]`, `corner = "top-left"` | its HUD panel's lines (`[]`: no panel) and corner |
| `told = ["Alex: Welcome!"]`, `broadcast = ["..."]` | server mods: every `players.tell` (as `name: text`) and `players.broadcast`, in order |
| `blocks = [{ x = 2, y = 64, z = 0, state = "minecraft:glowstone" }]` | server mods with `world`: those blocks of the Overworld after the step, the mod's `set-blocks` made (the script checks a state's shape; the game checks its name) |
| `entities = ["sentinel minecraft:iron_golem 2.5 64.0 0.5 hp=20"]`, `dealt = 2.0` | server mods with `entities`: the mods' mobs after the step as `tag kind x y z hp=health` (` dying` after it for a body), ordered by id; and the damage the step's `strike` dealt |
| `displays = ["item minecraft:nether_star x1 @ 0.5 64.6 0.5 riding 1000"]` | server mods with `displays`: their display entities after the step as `kind what @ x y z` (`text 8000 FE`, `block minecraft:redstone_block`, `item minecraft:nether_star x1`), ordered by name, ` riding <mob id>` after a riding one |
| `stopped = "panic"` | the mod was turned off, for a reason holding this text; without it, a mod that traps, panics or overruns fails the test |

Unknown keys are errors, so a typo never passes silently. The same runner is
`horizon_mods::testing` for Rust code that drives a mod (`TestGame`,
`Script`): Horizon's own tests run the examples' scripts with it. Scripts test
any mod, whatever it was written in.

### Template layout

```
my-mod/
├── Cargo.toml           # cdylib, horizon-mod-sdk, release: opt-level "s", LTO
├── horizon-mod.toml     # the manifest
├── src/lib.rs           # impl Mod, export_mod!
└── tests/hello.toml     # cargo horizon-mod test
```

A mod is a type implementing `horizon_mod_sdk::Mod`; its state lives in the
type:

```rust
use horizon_mod_sdk::{Mod, Registration, chat};

struct Hello { greeted: u32 }

impl Mod for Hello {
    fn init() -> Result<(Self, Registration), String> {
        Ok((Hello { greeted: 0 }, Registration::new().command("hello")))
    }
    fn command(&mut self, _name: &str, args: &str) -> Result<Option<String>, String> {
        self.greeted += 1;
        chat::show(&format!("Hello {args}! ({} so far)", self.greeted));
        Ok(None)
    }
}
horizon_mod_sdk::export_mod!(Hello);
```

Hooks not asked for keep their default bodies. The full example is
[`coords-hud`](../crates/horizon-mod-sdk/examples/coords-hud/src/lib.rs). Logic
that does not touch the API (formatting, maths) is ordinary Rust and unit-tests
on the host with `cargo test`.

### The manifest

```toml
id = "coords_hud"            # [a-z0-9_]{1,64}: settings key, log name
name = "Coordinates HUD"
version = "0.1.0"
description = "..."
authors = ["..."]
api = "0.1"                  # the API the SDK targets
side = "client"              # client | server | both
capabilities = ["game-read", "hud", "chat"]
experimental = []            # experimental capabilities it uses, e.g. ["ui"]
```

Unknown keys are refused, so a typo is an error, not a silently ignored
permission. An experimental capability (today `ui`, `render` and `input`) may change in any release
([stability](api/stability.md)): a mod uses it only by naming it in
`experimental` as well, so nobody depends on it by accident.

### Content mods (API 0.2)

A mod with `api = "0.2"` and capability `registries` may carry `assets/` and
`data/` in the `.hmod`, and needs no `mod.wasm` if it has no code
(`cargo horizon-mod new --template content` starts one; `build` in a folder
with no `Cargo.toml` packs the manifest, `assets/` and `data/`, and `check`
reads it without a trial run). It adds
blocks and items by declaring them in `data/<id>/horizon/block/*.json` and
`data/<id>/horizon/item/*.json`, each copying a vanilla template
([modpacks.md](modpacks.md#registries-02-data-driven-blocks-and-items)):

```json
{ "template": "minecraft:iron_block",
  "properties": { "hardness": 4.0, "light": 3, "sounds": "minecraft:amethyst_block" },
  "item": true }
```

The fence: files go in the mod's own namespaces (`assets/<id>/`,
`data/<id>/`, and those its manifest lists), plus additions to vanilla tags
(`data/minecraft/tags/`) and to the convention tags NeoForge and Fabric mods
share (`data/c/tags/`).
Templates and `sounds` name vanilla blocks; unknown keys are refused.

Some Java mods ship their files under other names than their mod id (Towns
and Towers, mod id `t_and_t`, under `kaisyn` and `towns_and_towers`). A
content mod lists those in its manifest:

```toml
id = "t_and_t"
api = "0.2"
capabilities = ["registries"]
namespaces = ["kaisyn", "towns_and_towers"]
```

A listed namespace is the mod's as its id is: its `assets/` and `data/`
files load, and the game reads its tags, recipes, names and loot. Blocks and
items are still declared under `data/<id>/horizon/` only. `minecraft`, `c`
and `horizon` cannot be listed. Two mods that are on may not own the same
namespace (one's id or listed namespace being the other's): the game keeps
the first in load order and leaves the other's content out, with
"mods A and B both use the namespace N; turn one of them off" in the log. Paths
use vanilla's resource characters, at most 65,536 files, 16 MiB each and
512 MiB in all. A mod whose content is refused stays off, with the reason on
its row. The game reads this content into its block and item tables
(`horizon_client::mod_content`, docs/modpacks.md). A tag entry naming an id
nobody registers (a block a port left out, another mod's item) is dropped,
so the rest of the tag still loads; vanilla would drop the whole tag.

Tags merge as a data pack's do. Every mod's file for one tag (vanilla's
`data/minecraft/tags/`, the convention `data/c/tags/`, or another mod's) adds
its values after vanilla's and the mods' before it, in load order. A file with
`"replace": true` first drops what came before it, vanilla's entries and the
earlier mods', and the mods after it still add theirs, as a higher data pack
does. Since the result depends on load order, the game says so in the log:
"mod towns replaces data/minecraft/tags/worldgen/biome/has_structure/village_plains.json:
vanilla's entries and those of adds are left out, ...". Tags are the only
`minecraft` files a mod may hold: its other content stays in its own
namespaces.

A mod adds to structure sets and template pools that are not its own
(vanilla's villages, another mod's houses) with a set or pool file in its own
namespace holding `horizon:add_to`, a key vanilla data never has. Its entries
are appended to each set or pool named, after every mod's own files, in mod
and file order; the file is no set or pool itself. A name nobody defines is
skipped.

```json
{ "horizon:add_to": "minecraft:villages",
  "structures": [ { "structure": "mymod:village_beach", "weight": 10 } ] }
```

A pool addition holds `"elements"` as a pool does. This is what lithostitched's
`add_structure_set_entries` and `add_template_pool_elements` modifiers do on
NeoForge and Fabric, and `horizon-port-pack carry` writes them from those.

A structure (and any of its pool elements, in place of the structure's) may
hold `horizon:enhanced_terrain_adaptation`: MoogsStructureLib's terrain
kernel, which carves or buries terrain around the
pieces on top of vanilla's `terrain_adaptation`. `kernel_size` and
`kernel_distance` shape the kernel; `top` and `bottom` (`carve`, `bury` or
`none`) act above and below a piece's floor; `bottom_offset`, `padding`
(`x`, `top`, `bottom`, `z`) and MoogsStructureLib's `band` (`bottom`, `top`,
`piece_heights`) move the box it measures from. On a structure,
`rigid_junctions` adds rigid pieces' junctions as well and
`inflate` grows the start's box by the kernel radius. `kernel_size` 0 is
none.

```json
{ "horizon:enhanced_terrain_adaptation": { "kernel_size": 24, "kernel_distance": 24,
    "top": "carve", "bottom": "none", "rigid_junctions": true, "inflate": true } }
```

### Server mods (API 0.3, experimental)

A server mod runs where the world is simulated: on the dedicated server, on a
LAN host and on single player's own server, never in a player's window. Its
manifest says `side = "server"` and `api = "0.3"` (and only a server mod may);
its component targets the WIT world `horizon-server-mod` instead of
`horizon-mod`. Experimental: it may change in any release
([stability](api/stability.md)).

| Interface | Capability | Functions |
|---|---|---|
| `log` | always | as for client mods |
| `storage` | `storage` | as for client mods, in the server's `horizon-mods/` |
| `players` | `players` | `online()`: id, name, position, dimension, game mode, health of each player on; `tell(id, text)`, `broadcast(text)`: system messages after the mod's name, 8 per call |
| `world` | `world` | `get-block(dimension, x, y, z)`: the state in a loaded chunk, `minecraft:oak_log[axis=y]`; `get-biome(dimension, x, y, z)`: `minecraft:plains`, ...; `set-blocks(dimension, changes)`: up to 4096 per call, checked at once, made as `/setblock` makes them (neighbours, light, block entities, every player told) after the call, before that dimension's next tick |

| Hook (export `server-events`) | When |
|---|---|
| `init() -> result<registration, string>` | once, after loading |
| `tick(tick)` | each server tick |
| `player-joined(player)`, `player-left(player)` | `registration.joins` |
| `chat(player, text)` | `registration.chat`: each chat message a player sends |
| `command(player, name, args, values)` | `/name args` typed by a player, for a command it registered, with its arguments' values (below); `ok(some)` is that player's feedback, `err` their red error |
| `block-broken(player, pos, state)`, `block-placed(player, pos, state)` | `registration.blocks`, with the state as `minecraft:oak_log[axis=y]` |

Capabilities that act on a local player (`game-read`, `chat`, `hud`) are for
client mods; `players` is for server mods. In Rust: `impl
horizon_mod_sdk::server::ServerMod` and `export_server_mod!`; the example is
[`welcome`](../crates/horizon-mod-sdk/examples/welcome/src/lib.rs), and with
`world`, [`beacons`](../crates/horizon-mod-sdk/examples/beacons/src/lib.rs).

**Typed commands.** Each command in `registration.commands` lists its
arguments, in order: a name and a kind, required or optional (optional ones
last). The server adds them to the command tree every player receives, after
vanilla's commands, so a player's chat box completes and colours them as it
does vanilla's (player names, coordinates, bounds); when a player sends one,
the server parses the line the way vanilla's parsers do and gives the mod the
values, by argument name, or answers with vanilla's error (`Integer must not
be more than 64, found 65`, `No player was found`) without calling the mod.

| Kind | Takes | Value | Chat box parser |
|---|---|---|---|
| `word` | one word: letters, digits, `_ - . +` | `word(string)` | `brigadier:string` (word) |
| `text` | the rest of the line, only last | `text(string)` | `brigadier:string` (greedy) |
| `integer(min, max)` | a whole number within the bounds given | `integer(s32)` | `brigadier:integer` |
| `number` | a number | `number(f64)` | `brigadier:double` |
| `boolean` | `true` or `false` | `boolean(bool)` | `brigadier:bool` |
| `player` | an online player's name, `@s` or `@p` (whoever typed it) | `player(player)` | `minecraft:entity` (one player) |
| `block-pos` | `x y z`, each a whole number, `~` or `~n` (from whoever typed it) | `block-pos(block-pos)` | `minecraft:block_pos` |

A command with no arguments takes nothing after its name. In Rust,
`Registration::command_with("beacon", [Argument::integer("height", Some(1),
Some(64)).optional()])`, read with `args.integer("height")`; `command(name)`
alone takes free text, an optional `text` argument named `args`
(`args.text()` is the line as typed either way).

**Capability `world`.** Reads are of the server's loaded chunks as they are
during the call: the game lends a server mod's call the levels' chunk maps
(moved, not copied, and only while a running mod has `world`), so a read
costs a map lookup and no copy of the world. A dimension with no chunk loaded
at a position reads as none. Writes go through the same code as `/setblock`,
so block updates, light, block entities and every client (LAN guests,
dedicated server players) see them as vanilla's; the parity harnesses test
that code. Changes for a dimension whose level isn't ticking wait for it
(in-frame single player ticks only the player's), up to 65,536 blocks; a
change in a chunk that isn't loaded when it is made is skipped, as `/setblock`
refuses one.

**Where they run.** The game that simulates the world starts the server mods
that are on, and only that one:

| Mode | Runs server mods | Players they see |
|---|---|---|
| Single player (default) | the server thread | the window's player, a connection like any other |
| Single player `--in-frame` | the window's own game, beside its client mods | the window's player |
| Open to LAN, Play with Friends | the host's server thread (or its window, in-frame) | the host and every guest |
| Dedicated server (`--dedicated`, `horizon-server`) | the server | every connected player |

A window that is a client of a server (single player's window, a LAN guest,
any multiplayer server) never runs them; its Mods page lists a server mod with
the others, and its switch takes effect the next time a world opens. The mods
live in `horizon-mods/` of the game folder, or of the server's folder on a
dedicated server, and are turned on as client mods are: Options > Mods, or
`mod.<id>:true` in `horizon-mods.txt`. `--dev-mod <file>` runs one always
on and reloads it when its file changes, on a server too.

**When hooks run.** At the end of each server tick the game tells the server
mods who joined and who left since the last one, then what players said,
broke and placed, in order, then calls `tick`. A command a player types
reaches its mod at once, whatever the player's permission level, and its
answer goes to that player. `tell` reaches one player, the window's own
chat for the in-frame player and a system message for a connection;
`broadcast` reaches every player and the server's log. With no server mod
on, each of these points costs one branch (measured in the PR that added
them).

**Not yet.** Block events are a player's own breaking and placing, not
explosions, pistons or other mods. Command arguments don't take entities,
items or block states yet, and `^` coordinates; in-frame single player's chat
box lists server mods' commands by name only (the server still parses their
arguments).

#### Block entities (capability `block-entities`, experimental)

A server mod keeps bytes of its own on each placed copy of the blocks it
claims: a machine's energy, a counter, an inventory it encodes itself. The
example `crates/horizon-mod-sdk/examples/cells` makes every lodestone an
energy cell.

```toml
capabilities = ["block-entities"]
experimental = ["block-entities"]
```

- **Claims.** `ServerMod::block_entity_blocks` names the blocks, once after
  `init` (up to 256, block ids without properties). Two mods may claim the
  same block: each keeps its own data on it.
- **Life.** A block entity starts empty when a player places a claimed block
  or a server mod sets one with `world.set-blocks`, and goes when the block
  there is no longer a claimed one, checked each tick in loaded chunks:
  `block_entity_removed` gets its last data. Unloaded chunks keep theirs.
- **Ticks.** `block_entity_tick` gets every block entity of the mod in loaded
  chunks each tick, after `tick`, with its state and data.
- **Data.** `block_entities::get` and `set` read and replace the data at a
  position, up to 16 KiB each and 65,536 block entities per mod. The game
  saves them with the world in `data/horizon/block_entities.json`.

**Not yet.** Blocks set by `/setblock`, pistons, explosions placing blocks or
structures don't make block entities; a server mod can't add blocks of its
own (a content mod can, and a server mod may claim them).

#### Energy (capability `energy`, experimental)

Energy buffers on block entities, shared by every mod: one mod's generator
feeds another's machine. The `cells` example keeps its charge this way.

- **Buffers.** `energy::set_storage` gives one of the mod's own block
  entities a buffer (capacity, most in and out per call), or changes it.
  `set_stored` sets what the mod's own buffer holds: a generator making
  energy, a machine spending it.
- **Between mods.** `get`, `insert` and `extract` work on any mod's buffer,
  within its limits; `simulate` only says how much would move. `push` moves
  energy from the mod's own buffer into the buffers on its six sides, shared
  evenly: a generator or a cable in one call.
- **Life.** A buffer goes with its block entity, after `removed` (which can
  still read it), and is saved with the world in `data/horizon/energy.json`.

**Not yet.** Item energy (batteries in inventories).

#### Fluids (capability `fluids`, experimental)

Fluid tanks on block entities, shared by every mod as energy is: one mod's
pump feeds another's machine, and `push` makes a block a pipe. Amounts are in
millibuckets (a bucket is 1000). The `cells` example gives each lodestone a
16-bucket tank (`/cellfluid [water|lava|drain|read] [pos]`) that keeps half a
bucket and passes the rest on to the cells beside it.

```rust
use horizon_mod_sdk::server::{TankSpec, fluids};

fluids::set_tanks(d, x, y, z, &[TankSpec {
    capacity: 16_000, max_receive: 1000, max_extract: 1000,
}])?;
fluids::insert(d, x, y, z, "minecraft:water", 1000, false); // 1000 went in
fluids::push(d, x, y, z, 0, 100); // tank 0 into the tanks beside it
```

- **Tanks.** `fluids::set_tanks` gives one of the mod's own block entities 1
  to 8 tanks (capacity up to 2^50 mB, most in and out per call), or changes
  them: a tank that stays keeps what it holds up to its new capacity. A tank
  holds one fluid at a time, named `namespace:path`: a vanilla one
  (`minecraft:water`, `minecraft:lava`) or the mod's own (`cells:plasma`).
  The game keeps no list of a mod's own fluids, so only the name's shape is
  checked. `set(tank, fluid, amount)` sets what the mod's own tank holds
  (`None` or 0 empties it).
- **Between mods.** `get` (all the tanks, `fluid` empty for an empty one),
  `insert` and `extract` work on any mod's tanks, within their limits.
  `insert` fills a tank that already holds the fluid before an empty one and
  never mixes fluids; `extract` names the tank and returns the fluid and the
  amount. `simulate` only says how much would move. `push(tank, amount)`
  moves fluid from the mod's own tank into the tanks on its six sides that
  take it, shared evenly, each as far as it takes: a pipe in one call. As
  with energy, an `insert` or `push` changes the other mod's tank at once.
- **Buckets.** `fluid_of_bucket(item)` and `bucket_of_fluid(fluid)` map the
  vanilla buckets (water, lava, powder snow) to their fluids and back; a
  bucket holds 1000 mB. A mod that fills and empties buckets does the item
  side with `items` or `item-hooks`.
- **The world's fluids.** Placing and taking a vanilla fluid in the world is
  not a call of its own: a pump reads the block with `world::get_block`
  (`minecraft:water[level=0]` is a source), removes it with `set_blocks` and
  `insert`s a bucket's worth; a placer does the reverse.
- **Life.** Tanks go with their block entity, after `removed` (which can
  still read them), and are saved with the world in
  `data/horizon/fluids.json`.

Test scripts need nothing new: the mod's own commands and ticks show the
tanks (the `cells` scenario `tests/fluids.toml` pours, flows, refuses a second
fluid and drains).

**Not yet.** Fluids in items (tanks on stacks, a bucket with a mod's fluid),
vanilla pipes or hoppers seeing the tanks, fluids placed in the world as
blocks of the mod's own fluid (a content mod can't declare a fluid yet), and
showing a tank in a menu as a gauge.

#### Settings (capability `config`, experimental)

Settings a server mod declares once and the player edits in a file while the
game runs: `config/<mod id>.toml` in the folder the mod is in (`horizon-mods/`).
The mod gives each entry a key, a description, a kind and a default; the game
writes the file with the descriptions and limits as comments, reads it again
when it changes, and puts back the default for a value that is missing or
wrong (saying why in the log).

```rust
use horizon_mod_sdk::server::{ConfigEntry, config, config_integer};

// In init: declared once; the file is written if it is missing.
config::declare(&[
    ConfigEntry::integer("charge", "Energy a cell makes each tick.", 5, 1, 1000),
    ConfigEntry::flag("announce-deaths", "Say in the chat when a player dies.", true),
    ConfigEntry::choice("fx-sound", "The sound /cellfx plays.", "chime", &["chime", "bell"]),
])?;

// Anywhere after: the value as the file holds it now.
let charge = config_integer("charge", 5);
```

```toml
# Energy a cell makes each tick.
# 1 to 1000. Default: 5
charge = 5
```

- **Entries.** `ConfigEntry::flag`, `integer(.., initial, min, max)`,
  `number(.., initial, min, max)`, `text` and `choice(.., initial, &[options])`;
  a key is `[a-z0-9_.-]{1,64}`, a description up to 512 bytes, up to 128
  entries, a text up to 1024 bytes, a choice has 1 to 32 options. `declare`
  refuses a default outside its limits, an empty choice or a key twice, and a
  second call replaces the entries.
- **Reading.** `config::get(key)` gives a `ConfigValue` (a choice is text);
  `config_integer`, `config_number`, `config_flag` and `config_text` give the
  value or a fallback. Reading is a plain call: read a value when the mod needs
  it, not into a copy that goes stale.
- **The file.** It is written when the mod declares and the file is missing or
  lacks an entry. A file whose entries are all there and right is never
  touched, so the player's own comments stay; when one is missing or wrong the
  file is written again in full (the player's own comments are lost). A whole number is accepted where a number is wanted. A value of
  the wrong kind or outside its limits is its default again, the file is
  written with it put right, and the log says which. A file that does not
  parse as TOML is left as it is (the mod keeps what it has) and the log says
  so once.
- **Edits.** The game looks at the file every five seconds. The mod hears
  `ServerMod::config_changed(keys)` with the entries whose value changed (a
  wrong value put back to its default counts when that changed it). It is not
  told of its own `config::set`.
- **`config::set(key, value)`** checks and writes, for a command or a screen of
  the mod's own (the `cells` example's `/cellconfig [key] [value]`). At most
  16 writes a call.
- **Test scripts** put new text in the config file with `config-file = "charge
  = 50
"` (the mod hears it at once, before `chat`) and check what the file
  holds with `expect.config-file = ["charge = 50"]` (each string must be in
  it). The file is there from the start, beside the mod's copy.

**Not yet.** A page in Options > Mods built from the entries (a client mod's
own `ui::set-settings` page is the way to show settings in the game), settings
per world (the file is for the whole game folder), settings for client mods
(`storage` and `ui` are theirs), and lists or tables as values.

#### Game events (capability `game-events`, experimental)

The server tells the mod what happens to players and the world, as a mod bus
does. They are notices after the fact: a mod that must change an outcome uses
the hook of the capability that owns it (`item-hooks`' `player_hurt` for
armour, `entities`' `entity_hurt` for a mod's mobs). A mod implements only the
ones it wants; a mod built before the capability has no such export and
keeps working. Within one tick the order of different events is not promised.

| `ServerMod` hook | Told when |
| --- | --- |
| `player_damaged(player, amount, health)` | a player's health fell, from any damage, whatever its source (not for the damage that killed them; healing is not told) |
| `player_died(player, cause)` | a player died; `cause` is the death message as vanilla writes it, empty when the `showDeathMessages` rule hides it |
| `player_respawned(player)` | a dead player is alive again |
| `dimension_changed(player, before, after)` | a player is in another dimension than the last tick |
| `game_mode_changed(player, before, after)` | a player's game mode changed |
| `mob_killed(player, kind, id)` | a player's **melee** blow killed a mob (a bow, a trident or an explosion are not told) |
| `weather_changed(weather)` | the weather turned: `clear`, `rain` or `thunder` |

```rust
fn player_died(&mut self, player: &Player, cause: &str) {
    players::broadcast(&format!("{} died: {cause}", player.name));
}
```

- **How they are found.** Health, dimension and game mode are compared once a
  tick for every player (the window's own player, LAN guests and a dedicated
  server's players alike); a death carries the message the game wrote for it.
  So a hurt is told once a tick at most, with the total lost in it.
- **Test scripts** tell the mods an event with `game-events = ["hurt Alex 4
  16", "died Alex fell from a high place", "respawned Alex", "dimension Alex
  minecraft:overworld minecraft:the_nether", "mode Alex survival creative",
  "killed Alex minecraft:zombie 7", "weather rain"]` (in order, after `net`
  and before `command`), and read what the mod did with `expect.told`,
  `expect.broadcast` and the other checks. The `cells` example says when a
  player dies, changes dimension or fells a mob, and when the weather turns.
- **No level change.** The capability adds no function a mod calls, so the API
  level stays where it was; an older Horizon refuses the mod's `game-events`
  capability by name instead.

**Not yet.** Damage sources and attackers for `player_damaged`, mobs killed
by ranged weapons, explosions or other mobs, mob spawns and despawns, item
pickups and drops, advancements, commands run by players, and events that a
mod can cancel (blocks broken and placed, chat and the hooks above already
exist as their own hooks).

#### Particles and sounds (capability `effects`, experimental)

A server mod shows particles and plays sounds in the world, for the players
near them. They are the game's own packets, so Horizon and vanilla clients
both get them, and in single player the window gets them as a guest of its
own server does.

```rust
use horizon_mod_sdk::server::{Particle, effects};

effects::spawn_particle(
    "minecraft:overworld",
    &Particle::new("minecraft:dust", x, y + 1.2, z, 20)
        .spread(0.3, 0.3, 0.3)
        .color(0x55FF55)
        .scale(1.0),
)?;
effects::play_sound("minecraft:overworld", "minecraft:block.note_block.chime",
    x, y, z, 1.0, 1.4, "block")?;
effects::play_sound_to(&player.id, "minecraft:entity.player.levelup", 1.0, 1.0, "player")?;
```

- **Particles.** `spawn_particle(dimension, particle)`: a name (`minecraft:flame`,
  `minecraft:happy_villager`, any particle the game has), a point, how far
  they scatter on each axis (0 to 64) and how fast they move (0 to 10), and
  how many (up to 500; with 0 one is sent that moves by speed times spread).
  The particles that need more take it from `color` (0xRRGGBB) and `scale`
  (0.01 to 4): `dust`, `dust_color_transition` (the colour twice),
  `entity_effect`, `effect`, `instant_effect`, `tinted_leaves`; or from `block`
  (`minecraft:oak_log[axis=y]`): `block`, `block_marker`, `falling_dust`,
  `dust_pillar`, `block_crumble`. Players within 32 blocks get it
  (`always_show`: within 512, whatever their particle setting says).
  Items, trails, vibrations and flashes are not sent. A particle the game does
  not know, or one missing its colour or block, is dropped and logged.
- **Sounds.** `play_sound(dimension, sound, x, y, z, volume, pitch, category)`
  plays in the world: volume 0 to 10 (it carries 16 blocks a unit, at least
  16), pitch 0.5 to 2 (clamped), `category` one of `master`, `music`, `record`,
  `weather`, `block`, `hostile`, `neutral`, `player`, `ambient`, `voice`: the
  slider the player hears it on. `play_sound_to(player, ..)` plays for one
  player at their own position (a cue, not a place in the world). A sound the
  game does not know is sent by name and played when the player's resource
  pack has it.
- **Limits.** 128 effects a hook call (more is an error). Only the dimension
  the server is simulating is heard; others are dropped.
- **Test scripts** check what was sent with `expect.effects`, in order:
  `particle minecraft:dust x20 @ 0.5 64.2 0.5`, `sound
  minecraft:block.note_block.chime 1 0.5 block @ 0.5 63.5 0.5`, `sound-to Alex
  minecraft:entity.player.levelup 1 1 player`. The `cells` example's
  `/cellfx` puffs dust and chimes, and a sentinel's fall smokes and sounds.

**Not yet.** Sounds of the mod's own (a content mod's sound files and
`sounds.json` are not wired to this: untested), sounds that follow an entity, stopping
a sound that is playing, particles with items, trails or vibrations, and
effects in other dimensions.

#### Redstone (capability `redstone`, experimental)

Redstone for the mod's own blocks, both ways. The game reads the signal on
each face of every block entity the mod owns, once a tick, and tells the mod
when it changes; the mod sets the signal each face gives out, and vanilla
blocks around it (wire, lamps, repeaters, comparators, doors, pistons, note
blocks) read it as they read a redstone block or a lever. Signals are 0 to
15 on the faces `down`, `up`, `north` (towards -Z), `south`, `west` and
`east`; `Sides::all(p)` and `Sides::strongest()` help.

```rust
use horizon_mod_sdk::server::{RedstoneOutput, Sides, redstone};

// Giving out: a gauge of 7 on every face.
redstone::set_output(d, x, y, z, RedstoneOutput::weak(7))?;

// Reading, in ServerMod::redstone_changed(dimension, x, y, z, &Sides),
// or any time with redstone::get_input(d, x, y, z).
```

- **Reading.** `ServerMod::redstone_changed(dimension, x, y, z, input)` runs
  for each of the mod's block entities when what it reads on a face changed,
  when it is read for the first time (a block entity placed, the world or the
  mod started: so a mod need not poll), and when a neighbouring block
  changed, at most 256 a tick. `input` is, on each face, the signal the
  neighbour there gives it: wire, a lever, a redstone block, a repeater
  facing it, another mod's block. `redstone::get_input` reads the same any
  time, as of the start of the tick; a block entity is read from the tick
  after it appears. Only blocks the mod owns can be read.
- **Giving out.** `redstone::set_output(.., RedstoneOutput { power, strong })`
  sets what the block gives on each face (all zeros stops it): what the block
  touching that face reads. `RedstoneOutput::weak(p)` is a redstone block's
  kind of signal; `strong: true` (`RedstoneOutput::strong(p)`) also powers
  the blocks it touches through, as a lever powers the block it is on. The
  blocks around it update when it changes, as they do for a lever. The
  signal is saved with the world in `data/horizon/redstone.json`, so it
  stands after a restart before the mod has run, and goes with its block
  entity. Another mod's block is read-only: `get_output` says what it gives.
- **The `cells` example** gives out a gauge of its charge (a level for each
  1000 FE) on every face, so a lamp or wire beside it shows it, and stops
  charging while a signal reaches its top face (it reads the top only: the
  cells beside it give out gauges of their own).
- **Test scripts** put a signal on a block's face with `signals = [{ x, y, z,
  side = "north", power = 15 }]` (standing until a step changes it) and check
  what blocks give out with `expect.redstone = ["0 63 0 1 1 1 1 1 1"]` (`x y
  z down up north south west east`, ` strong` after a strong one).

**Not yet.** Wire and repeaters do not visually connect to a mod's block (a
redstone block does), comparators read no container of the mod's, no
quasi-connectivity, and mobs' own pathfinding doesn't see a mod's signal.
Only the dimension the server is simulating is read and driven.

#### Menus (capability `menus`, experimental)

Chest menus a server mod opens for a player: its own items in the slots,
named and described as it likes, a click on each reaching the mod. Every
client shows them, vanilla's too: they are vanilla's chest menus
(`minecraft:generic_9x1` to `9x6`). The `cells` example's `/cellgui` shows a
cell's charge, refreshed twice a second, with buttons to drain and fill it.

- **Open.** `menus::open(player, menu, title, rows, items)`; `menu` is the
  mod's own name for it. `MenuItem::new(slot, item, count).named(name,
  lore)` builds a slot. Opening closes the menu the player had open.
- **Clicks.** `menu_clicked(player, menu, slot, button, shift)` for a left or
  right click (shift-click included) on the menu's own slots. The player
  can't take or drop the items: the menu shows the mod's items again after
  every click.
- **Change and close.** `set_items` replaces the slots of the open menu;
  `close` closes it. `menu_closed` follows the player closing it or leaving.

**Not yet.** Slots players put their own items in; menus for the window's
own player in in-frame single player (the default single player, on the
server thread, has them).

#### Inventories (capability `items`, experimental)

Slots on block entities, shared by every mod as energy is (`IItemHandler`'s
place): `items::set_slots` gives one of the mod's own block entities up to
54 slots; `get`, `insert` (equal stacks first) and `extract` work on any
mod's inventory, `set` on the mod's own. Stacks are an item, a count and
components in vanilla's item syntax. Inventories are saved with the world in
`data/horizon/items.json`; when their block goes, the items drop there
(after `removed`, which can still read them), and so do those of slots
`set_slots` takes away.

`menus::open_inventory` opens a menu whose listed slots show such an
inventory, each `Storage`, `Input` (players only put items in) or `Output`
(only take them out): clicks pick up and place as in a chest (right click:
half, or one), shift-click moves stacks between it and the player's
inventory, and the menu follows the inventory as the mod changes it. A
`cells` lodestone turns redstone put in its left slot into energy, and its
Drain button makes redstone in its right one.

**Not yet.** Drags across slots, number-key swaps and double-click
collecting in mod menus (the menu shows the inventory again after them);
hoppers and other vanilla blocks moving items in and out.

#### Blocks of a mod's own (`block-use-events`)

A server mod with block entities makes its claimed blocks machines of its
own: `ServerMod::block_uses` names those whose use (a right click) it takes,
and `block_used(player, block)` hears each, with the block's data. The game
does nothing else with such a click: no vanilla use, no block placed against
it; a sneaking player with an item in hand still places or uses it as
vanilla does. A right click on a `cells` lodestone opens its menu.

#### Displays (capability `displays`, experimental)

Block models, item models and text a server mod shows in the world, not as
blocks: `displays::show(name, dimension, x, y, z, shape, transform,
interpolation)` shows the mod's display `name` or changes it, `hide(name)`
removes it. `Shape::Block("minecraft:redstone_block")` draws a block's
model from the position as its corner; `Shape::Text(text)` floats text
that faces whoever looks at it; `show_item(name, dimension, x, y, z,
&DisplayStack::new("minecraft:nether_star"), transform, interpolation)`
draws an item's model — the stack is a look, not storage (count 1 to 99,
components up to 512 bytes in vanilla's item syntax).
`Transform::identity().scaled(..).turned_y(..).translated(..)` moves,
scales and turns it; the client blends to a new transform over
`interpolation` ticks, so a display updated every few ticks moves smoothly.
`move_(name, x, y, z, interpolation)` moves a display of any shape without
remaking it — the client walks it there over `interpolation` ticks (at most
59), so an orbit updated every few ticks never flickers; a display riding
a mob is put down first. `ride(name, Some(entity))` makes a display ride
one of the mod's mobs (capability `entities`), carried along: it sits
where the mob stands, lifted by its own `transform.translation`;
`ride(name, None)` puts it down where it stands. At most 16 displays ride
one mob.

Each is a vanilla display entity (`block_display`, `item_display`,
`text_display`) sent to the players in its dimension (clients joining later
get it too); it lasts while the mod runs and is never saved — a mod
restores what it needs after a restart (the cells example re-shows its
stars and re-rides them in `entity_tick`, where the mobs' new ids are).
Up to 1024 per mod. Above each `cells` lodestone float its charge, a
spinning redstone core and four nether stars that circle it; a sentinel
carries a star above its head.

**Not yet.** Free model files (Blockbench/GeckoLib): a display is a block,
an item or text.

#### Machine recipes (capability `machine-recipes`, experimental)

The recipes of a server mod's own machines, in its `data/<id>/recipe/` files:
the game loads them once, before the mod starts, so its `init` already reads
them. Each file is one recipe — its `type` (as `cells:grinding`), its
`inputs` and `results` in vanilla's ingredient syntax (`"minecraft:redstone"`,
`"#c:ores"` (a tag, expanded at load), `["a", "#b"]` (any of them),
`{"item": "..", "count": n}`, `{"tag": ".."}`; `input`/`ingredient` and
`result`/`output` name a single slot) — and whatever else the mod's own
fields are: the whole file reaches the mod as `data`, to parse as it knows
(energy, ticks, tiers). The files come from the mod's own package (`.hmod` or
folder) and from a content mod's carry of the same id (`horizon-forge
convert` lays one down); the package's file wins. At most 512 recipes a mod
and 16 KiB a file; a mod with this capability may carry only these files,
under its own id.

`machine_recipes::all(kinds)` lists the mod's recipes of those kinds (an
empty list: every one; at most 16 kinds) — call it once in `init` and keep
the answer, not each tick. `machine_recipes::by_input(stack, kinds)` narrows
it to the recipes a slot of whose `inputs` takes the stack's item (a free
slot takes anything): a machine's tick asks what it can make now, with at
most 64 back. Neither changes while the mod runs, and neither errors — an
empty list is the honest answer. A `cells` lodestone grinds cobblestone and
andesite in its input slot into redstone this way (`data/cells/recipe/` in
the example), and its `/cellrecipes` counts the recipes.

**Not yet.** Changed recipe files are read at the next start of the mods (a
dev rebuild of the code alone keeps the table as it was); tags are expanded
once, at load, and a tag naming no item skips its recipe; the mod's id is the
only namespace — unless it also has `registries`, whose manifest may list
others; recipes are not per-world and never saved with one, being content;
`by-input` matches items, not components.

#### Multiblocks (capability `multiblocks`, experimental)

The structures of a server mod's own multiblocks, in its
`data/<id>/multiblock/` files: the game loads them once, before the mod
starts, as machine recipes are. Each file is one structure — its `layers`
(a list from the bottom up, each `depth` rows of `width` characters, x
along a row and z down them), its `palette` (a character → a block id, a
list of them (any fits), `"air"` for a cell that must be empty, `"any"`
for one not checked; a space is always air) and its `anchor`, the shape's
cell the mod's block entity sits on (`[0, 0, 0]` by default) — and
whatever else the mod's own fields are: the whole file reaches the mod as
`data`. The files come from the mod's own package and from a content
mod's carry of the same id; the package's file wins. At most 64
structures a mod, 4096 checked cells and 32 a side in one, 16 blocks a
cell and 16 KiB a file; a mod with this capability may carry only these
files (and its recipes), under its own id.

```json
{
  "anchor": [1, 1, 1],
  "palette": { "#": "minecraft:iron_block", "C": "minecraft:lodestone", ".": "air" },
  "layers": [ ["###", "#.#", "###"], ["###", "#C#", "###"], ["###", "#.#", "###"] ]
}
```

`multiblocks::all()` lists the mod's structures — call it once in `init`
and keep the answer, not each tick. `multiblocks::watch(dimension, x, y,
z, id)` asks the game to follow the copy of structure `id` whose anchor
is the mod's block entity at a position: about once a second the game
checks it, calling `multiblock_formed` when it has come together and
`multiblock_broken` when it no longer holds (or its anchor's block went,
with the block entity's last data). The answer is the quarter turn it is
whole at as of the call — the world is lent in the server's hooks, so a
mod watching in the first `block_entity_tick` of an anchor learns at once
whether the structure already stands; after a restart of the mod or the
game it is the answer to trust, the events not repeating — and the
watch's state is kept across saves. Err where the mod has no block
entity at the position, no such structure, or it watches over 256. A
first watch of an anchor in its `block_entity_tick`, and `formed`/
`broken` broadcast or mark the mod's own state: an energy core, a fusion
chamber ready. Several structures may share one anchor (a core's tiers):
watch each id, and `formed` names which came together. `multiblocks::check(dimension, x, y,
z, quarter, id)` matches structure `id` turned by `quarter` quarter turns
clockwise looked at from above (y doesn't turn) against the world with
its anchor at a position: `Ok(None)` — it is whole; `Ok(Some(..))` — the
first mismatches, at most 16, each where in the world, the block the
template takes there and what is there; `Err` for a structure the mod
doesn't carry, a quarter past 3, a position whose chunk isn't loaded, or
no world lent — it is a read of the server's world, and answers in the
server's hooks (a command, `block_used`, `block_entity_tick`), not in
`init`. Fusion crafting works this way: try each quarter, and the first
mismatch tells the player what to place. A structure is anchored on one
of the mod's own block entities — the manifest needs capabilities
`block-entities` and `world` beside `multiblocks` — and a check is only a
read: to change blocks is `world.set-blocks` as always. The `cells`
example's `/cellcore [pos]` says its core is whole or names the first
block it needs (`data/cells/multiblock/` in the example).

**Not yet.** Block states (properties) are not matched, only block ids,
and a palette names no tags; turns other than quarter ones, and mirrored
shapes, are not searched (`check` answers one quarter a call, and a watch
tries them in turn itself); a followed structure is checked about once a
second, not on every neighbour's change — a block placed and taken away
between two checks is never seen, and no event tells the mod which
neighbour changed; structures without one of the mod's block entities as
their anchor cannot be watched; changed files are read at the next start
of the mods, as machine recipes are.

New blocks come from a content mod (API 0.2, capability `registries`) that
a server mod claims, or from the server mod itself (capability `registries`
on API 0.3, [below](#a-server-mods-own-content-capability-registries)):
`horizon-forge convert` carries a Java mod's blocks,
models and textures over as one.

#### World data (capability `saved-data`, experimental)

Bytes a server mod keeps on the world itself, not tied to a block: the
members of an energy network, who owns a claim, a list of teleports. The
mod's `storage` folder (capability `storage`) is the mod's own, beside the
mods; this is the world's, in `data/horizon/saved_data.json`, and travels
with the world folder — whoever opens it next, the mod reads back what it
wrote. The `cells` example keeps its power grids this way (`/cellnet`).

```toml
capabilities = ["saved-data"]
experimental = ["saved-data"]
```

- **Keys and values.** `saved_data::get(key)` and `set(key, data)` read and
  replace the bytes at `key` of the mod's own, `delete(key)` removes it,
  `keys()` lists the keys in order, so a mod can walk its networks without
  an index key of its own. Values are bytes; how they are encoded is the
  mod's business (its own text, postcard, bincode).
- **Limits.** Keys are `[a-z0-9_.-]{1,64}`, one path segment, as
  `storage`'s. At most 64 KiB a key, 256 keys and 1 MiB a mod — the file
  is rewritten with every save of the world, so the quota is smaller than
  `storage`'s on purpose. For big data, `storage`; for data that must
  travel with the world, this.
- **When.** Read and written only during the server's hooks, as the block
  entities are: a call outside them is an error, not a crash. Changes
  reach the file when the world is saved, with it.
- **Whose.** Each mod sees and changes only its own data, whatever key it
  guesses; a disabled or missing mod's data stays in the file and is back
  when it is enabled again.

**Not yet.** No telling of another mod's changes, no versioning or
migrations of the bytes, no sharing between mods; values are raw bytes,
not for large data.

#### Items with behaviour (capability `item-hooks`, experimental)

A server mod takes the behaviour of the items it claims, by id — a
vanilla item (`minecraft:blaze_rod`) or a content mod's (`mymod:wrench`),
as blocks are claimed for block entities. It hears their use and their
blow, each tick they are held or worn, and it keeps bytes of its own on
each stack (energy in a tool, its mode): a component on the stack, so the
bytes travel with it between slots, players and the world's saves — no
file of their own. The `cells` example claims a blaze rod as an energy
staff and a golden helmet as a shield (`/staff`).

```toml
capabilities = ["item-hooks"]
experimental = ["item-hooks"]
```

- **Claims.** `ServerMod::item_hooks_items` names the items, once after
  `init` (up to 256, ids with a namespace). Two mods may claim the same
  item: each hears its hooks on it and keeps its own bytes on the stack.
- **Use.** The player used (right-clicked) a claimed item in the main
  hand: `item_used` with nothing to use it on, `item_used_on_block` with
  the crosshair on a block, after the block's own vanilla use passed (a
  door opened still swallows the click). The mod hears it at the tick's
  end, as a block's use; the game does nothing else with the click — but
  a sneaking player keeps vanilla's use, the off hand is always
  vanilla's, and a block a mod takes the use of (capability
  `block-entities`) wins over the item.
- **Blow.** The player struck an entity — a mob, or a player in PvP —
  with a claimed item in the main hand: `hurt_entity` answers before the
  damage lands, with what the blow should deal instead (0 to cancel it;
  the same damage to change nothing), kept to nothing and four times
  vanilla's. Only the number changes: knockback, sweep and the rest are
  vanilla's. An error, or a mod that traps, cancels nothing.
- **Tick.** `inventory_tick` gets every claimed stack of every online
  player each server tick, after the block entities' tick — held or
  worn, armour's slot named (36 feet, 37 legs, 38 chest, 39 head). At
  most 512 a call: a longer list comes in turns, the game walking it
  round-robin by the tick, so every stack is told within a few ticks.
- **Armour.** A player wearing a claimed item was hurt for a typed
  damage (`minecraft:fireworks`, `minecraft:magic`): `player_hurt`
  answers with how much of it the mod's armour takes — what it takes is
  gone, the rest hurts as vanilla has it. The hook hears the hurts the
  game funnels by their type: falling, firework rockets, magic and
  poison effects, an ender pearl, the dragon, lightning, thorns.
- **Data.** `item_hooks::stack(player, slot)` reads a claimed stack —
  its item, count and the mod's own bytes on it — and
  `set_data(player, slot, data)` replaces them (none empties them):
  41 slots a player, the 36 inventory slots (hotbar first), armour feet
  to head 36..39, the off hand 40. Both ask for a claimed item, at most
  4 KiB and 32 writes a call, and a stack of one: a split copies a
  stack's components to both halves, so charge on a splittable stack
  would double — a stack that carries data keeps count 1, and what the
  bytes hold is the mod's business (the example keeps a `u64`
  little-endian). Read and written only during the server's hooks, as
  the block entities are; a vanilla client doesn't see the component,
  and the player's own client is told each change.

**Not yet.** The left hand; breaking blocks with a tool of the mod's
(durability, efficiency — game-loop events); status effects while worn
(an armour mod has the tick and its take instead); the item's name or
lore from its data (a charge shown in the name); damage the game funnels
through its full hurt pipeline (mob melee, arrows, PvP) reaching
`player_hurt`; AoE blows; a cooldown of the mod's; data on stacks that
split (see above).

#### Network and screens (capability `network`, experimental)

The two halves of one mod talk: a **server mod** and a **client mod** that
share an id (two files, `side = "server"` and `side = "client"`; turning the
id on in Options > Mods turns both on, each running where it belongs: the
client half in the player's game, the server half where the world is). A
message is bytes on a channel of the mod's own (`cell/state`, `[a-z0-9_./-]`
up to 64 characters, up to 30,000 bytes); its format is the mod's business.
With the client half's `ui` widgets this is a real screen, not a chest.

```toml
# server half                        # client half
side = "server"                      side = "client"
api = "0.3"                          api = "0.1"
capabilities = ["network"]           capabilities = ["ui", "network"]
experimental = ["network"]           experimental = ["ui", "network"]
```

- **Server to client.** `network::send_to(player_id, channel, bytes)` in a
  server mod; the client mod hears it in `Mod::network_received(channel,
  bytes)`. The player must be a Horizon client (vanilla ones get an error).
- **Client to server.** `network::send(channel, bytes)` in a client mod; the
  server mod hears it in `ServerMod::client_message(player, channel, bytes)`,
  at the end of the server's tick. A player sends at most 32 messages a
  tick; more are dropped. **The server must not trust what a client sends**:
  keep what the server showed each player on the server (the `cells`
  example remembers which cell it opened for them, and ignores any position
  in a message) and check every request.
- **Limits.** 64 messages and 256 KiB a call; a message to a vanilla server
  or from one is simply not sent. Messages travel as one custom payload
  channel (`horizon:mod_net`) in the play state, announced by the server in
  configuration, so they need nothing from a vanilla client and work in
  single player (the window is a client of its own server), on a LAN and
  on a dedicated server.
- **A screen.** The client half opens a screen with `ui::open_screen` when a
  message says so, and calls it again with new widgets for each update (a
  second call replaces the screen); a button press arrives as
  `Mod::ui_select(id, 0)` and Esc as `ui_closed`, and the mod sends the
  press to the server, which decides what it does and answers with the new
  state. The `cells` example's `/cellui` opens such a screen: a cell's
  charge as text and a bar, its redstone as an item row, and two buttons.
  Test scripts have a `net` step (a message of the other half), `select` and
  `ui-close` (a press, Esc), and `expect.net` and `expect.screen`.

**Not yet.** Telling whether a player's client has the mod's half (a screen
opened for one without it just doesn't show: `/cellgui`'s chest menu is the
fallback); the half running in in-frame single player (the server mod and
its client half would share an id in one game); screens beyond the `ui`
widgets (text, buttons, item rows, lists: no free layout, textures or
sliders); mod messages through a proxy that drops unknown payloads.

#### Mobs of the mod (capability `entities`, experimental)

A server mod spawns mobs of its own: **vanilla mobs the mod marks and then
steers** (a zombie, an iron golem, a cow — any mob kind the game knows). The
game runs them as it runs every mob: hitbox, physics, damage, knockback,
loot, saving. The mod chooses what they are and what they do. This is how a
mod gets a guardian, a drone or a boss without a mob model of its own: pick
the closest vanilla body, switch off its AI (`no_ai`), hide it (`invisible`)
and put display models over it (capability `displays`), or leave it be.

```rust
use horizon_mod_sdk::server::{entities, ModEntity, SpawnOptions};

let id = entities::spawn(
    "minecraft:overworld", x, y, z,
    &SpawnOptions::new("minecraft:iron_golem", "sentinel")
        .takes_use()
        .named("Sentinel"),
)?; // its entity id, known at once
```

- **Spawning.** `entities::spawn(dimension, x, y, z, options)` returns the
  mob's entity id, known at once; the mob exists from the end of the call.
  `SpawnOptions::new(kind, tag)` is a vanilla kind (`minecraft:zombie`) and
  the mod's own name for it (`[a-z0-9_./-]`, up to 64 characters), then
  `.no_ai()` (no goals and no movement of its own: the mod moves it),
  `.invisible()`, `.silent()`, `.invulnerable()`, `.no_gravity()`,
  `.takes_use()` (a right click reaches `entity_used` and does nothing
  else, unless the player is sneaking), `.named(text)` (shown over it) and
  `.health(h)`. A mod has at most 1024 mobs and spawns 32 a call. A spawn
  fails for an unknown kind, a bad tag, another dimension than the one the
  server simulates (for now), and **in the hooks the game runs mid-tick**
  (`entity_hurt` and the item hooks' blows): the error says so. Everything
  else spawns: commands, ticks, joins, menu clicks, block events.
- **Steering.** `entities::get(id)`, `list()` (the mod's own mobs, as of the
  start of the call), `teleport(id, x, y, z)` (the way to move a mob with
  its AI off), `push(id, x, y, z)` (adds a velocity), `set_health(id, h)`
  (0 kills it) and `remove(id)` (no death, no loot). A mod acts on its own
  mobs only; the rest are an error.
- **Events.** `ServerMod::entity_tick(tick, mobs)` each tick (up to 512
  mobs, the first by id), `entity_hurt(player, mob, damage)` when a
  **player's** blow is about to hurt it (answer with the damage to deal: `0`
  cancels, never more than four times; an error cancels nothing, other
  damage lands as vanilla has it), `entity_died(mob)` once as it starts to
  fall (whatever killed it), `entity_removed(mob)` when it is gone (after
  its death, removed or despawned; its last state) and `entity_used(player,
  mob)` for a right click on a mob that takes the use. Each mob a hook gets
  carries its id, kind, tag, dimension, position, yaw, health and full
  health, and whether it is `dying`.
- **Marks.** The game finds a mod's mobs by marks in the mob's saved data
  (`Tags`: `horizon_mod:<id>`, `horizon_tag:<tag>`, `horizon_use`), so the
  mod hears its mobs again after a restart. A mod that is turned off leaves
  its mobs standing as plain mobs; they are its again when the world is
  next loaded with it on.

The `cells` example's `/sentinel` spawns an iron golem, tag `sentinel`,
named, that takes a third of a player's blow (`entity_hurt`), mends a point
of health a second (`entity_tick` and `set_health`), answers a right click
with its health and tells the server when it falls.

**Not yet.** Hooks for damage that is not a player's blow (fire, a fall,
another mob's attack); mobs of other dimensions than the one the server
simulates; the mod's own AI goals (it moves a mob with `teleport` and `push`
each tick); mobs that are not vanilla kinds (a model of its own: use display
models).

#### A server mod's own content (capability `registries`)

A server mod may carry the same `assets/` and `data/` a content mod does,
in the same `.hmod` as its code: `api = "0.3"`, `side = "server"`,
`capabilities = [..., "registries"]` (stable: no `experimental` entry).
Everything the content fence allows a 0.2 mod
([Content mods (API 0.2)](#content-mods-api-02)) is allowed it too, under
the same caps: blocks and items declared in `data/<id>/horizon/`, files in
its own namespaces (its id and those its manifest `namespaces` list — the
common rules, not a server-mod restriction), and additions to vanilla and
convention tags. `cargo horizon-mod build` packs code and content into the
one file; a mod whose content is refused stays off, with the reason on its
row.

How it differs from a content mod:

- The mod has code: `mod.wasm` is there beside the manifest, and no second
  content mod of the same id is needed to carry the files (the carry of
  [Machine recipes](#machine-recipes-capability-machine-recipes-experimental)
  stays for server mods without `registries`).
- The content loads in every window that has the mod on — including a pure
  client window where the mod's code does not run (`side = "server"`). That
  is the multiplayer path: a Horizon client with the same `.hmod` builds
  the same overlay, so the state ids match the server's. The ids are
  deterministic (vanilla's states, then the on mods' blocks in id order),
  so both ends agree.
- The mod's blocks live in a saved world as global state ids, as a content
  mod's do: a world opened without the mod has unknown ids where they
  stood.

The id of a declared block is `<mod id>:<name>`, which the mod knows
statically — it wrote the file: the `cells` example declares `cells:cell`
(`data/cells/horizon/block/cell.json` in the example), a lodestone's
behaviour under its own id, and claims it like the vanilla block by naming
it in `block-entity-blocks` and `block-use-events`.

**Not yet.** A vanilla client does not see a server mod's blocks (registry
sync is not built: a Horizon client needs the same `.hmod` on); a mod
cannot ask the game for the ids of its own declared blocks — it is the
author of `data/<id>/horizon/block/`, so it knows them, and a claim naming
a block nobody declares simply never fires.

#### Library mods and messages between mods (capability `inter-mod`, experimental)

Server mods message each other, so the code one mod carries can serve them
all — what modding platforms call a library (a BrandonsCore, a Curios, a
GeckoLib): written once, declared in a manifest, started before whoever
needs it. A message is bytes on a channel of the sending mod's own naming
and format, exactly `network`'s, and never leaves the game that simulates
the world: nothing of this goes over the wire, so a vanilla client and a
vanilla server see nothing of it.

```toml
# the library                            # the mod that needs it
api = "0.3"                              api = "0.3"
side = "server"                          side = "server"
capabilities = ["inter-mod", "players"]  capabilities = ["inter-mod", "players"]
experimental = ["inter-mod"]             experimental = ["inter-mod"]
provides = ["spin-lib@1.0"]              requires = ["spin-lib@1"]
```

- **Provides and requires.** The game starts providers before the mods
  that need them (with no `requires`, the load order is the mods' own, as
  always), and keeps a mod off, with the reason on its Mods page row,
  whose need no turned-on mod provides: "needs spin-lib@1; no turned-on
  mod provides it". A need is served by the same name and the same major
  version at least as new — `1` takes any `1.x`, `1.2` takes `1.2` and
  later in the 1 line, and `2` never serves `1`; a circle of requires
  keeps them all off, named as such. The gate looks at turned-on mods
  wherever they run, so a server mod pair both on passes it in the
  window's game too.
- **The bus.** `inter_mod::expose(name, version)` registers the mod as
  serving an API — one entry of its manifest's `provides`, checked — and
  `inter_mod::providers(name)` lists who serves it, their ids and
  versions, as of the call; a library turned off is not in it. At most 8
  APIs a mod. `inter_mod::subscribe(channel)` hears a channel (at most 16
  a mod), `inter_mod::publish(channel, data)` reaches every other mod
  subscribed to it, and `inter_mod::send_to(mod_id, channel, data)`
  reaches one running mod however it gets the channel — which must hear
  messages (a send to a mod without the `inter-mod-events` export, or to
  itself, is an error). Channels are `[a-z0-9_./-]` up to 64 characters,
  messages up to 30,000 bytes, 64 messages (256 KiB) a call.
- **Delivery.** Messages wait for the end of the server's tick, then go
  out in rounds: an answer a library sends while hearing a message is
  delivered by the next round of the same tick, so a request and its
  answer fit one tick, four hops deep. A mod is handed at most 64 messages
  (512 KiB) a tick — the rest wait for the next one — and never a message
  it sent itself. A mod that stops, or is reloaded, is taken off the bus:
  its exposures and subscriptions go, and it re-exposes and re-subscribes
  as it starts again; nothing of the bus is saved with the world.
- **Hearing.** `ServerMod::message(from, channel, data)` is the call, with
  the sender's id. **A mod must not trust what another mod sends**: the
  bus carries bytes between mods that chose to talk, and a broken or
  hostile mod on the same server can send anything — check every message,
  as the server must what a client sends (`network`'s rule).

The `spin-lib` and `spin-bell` examples are the pair: the library exposes
`spin-lib` at `1.0` and answers a counter on `spin/tick` with the angle it
stands for on `spin/angle`; the consumer requires `spin-lib@1`, publishes
its counter each tick and rings at the first answer — within the same
tick, by the rounds. Alone, spin-bell stays off with its reason; its test
script runs only beside the library (`TestGame::load_pair` in the
sandbox).

**Not yet.** A typed import of a library's own interface (component
composition, the consumer calling the library's functions directly); a
synchronous call (a question that blocks for its answer — the rounds
deliver within the tick instead); client mods on the bus (they have
`network`); asking the bus what channels exist.

## Publish your mod

Horizon's registry lists mods, modpacks (`.hpack`) and resource packs made for Horizon, each reviewed before it is
published. Players find them at [playhorizon.dev/mods](https://playhorizon.dev/mods) and in the launcher (Mods, then
**Horizon** beside Modrinth), which checks every file before it installs it.

1. Run `cargo horizon-mod check` on the file (above): the registry runs the same checks.
2. Put the source somewhere public, under a licence that lets players run it: reviewers read it, and the file must be
   built from it.
3. Open [playhorizon.dev/mods/submit](https://playhorizon.dev/mods/submit), sign in with GitHub (it reads only your
   public profile) and fill in the page: the file, its name and summary, licence, source address, and optionally a
   minisign public key with the file's `.minisig`.
4. The form checks the file again and opens a pull request in
   [horizon-rust/horizon-mods](https://github.com/horizon-rust/horizon-mods) that adds `mods/<id>.toml`, with you as its
   owner. That repository's checks run the file in the sandbox and post their report on the pull request; answer the
   reviewers there.
5. A reviewer merges it: within minutes the file is a release of horizon-mods, and the project is on the site and in
   the launcher.

Once it is published, its page and its card in the launcher show its downloads (GitHub's counts of its files,
refreshed every hour) and its rating: players signed in with GitHub on its page give it 1 to 5 stars, one rating per
account, which they can change. Its owners can't rate it, and nobody sees who rated.

An update is the same form with a new version; only the project's `owners` can publish one, signed with its key if it
has one. A published file never changes. Without the form, open the pull request by hand (the repository's README
says how). The formats: [api/mod-index.md](api/mod-index.md).

### Horizon's official mods

The Horizon extras that are mods now, and Horizon's own recipe viewer, live in `mods/official/<name>/`, one crate each in the game's workspace, so
`cargo clippy --workspace` and pr_ready cover them. They use only the public API, like any other mod, with the same
options on their settings page as the extra had and the same look. Each is published by a maintainer: build it on
`main` with the toolchain CI uses (`cd mods/official/<name> && cargo horizon-mod build`), then open a pull request in
horizon-mods with its `mods/<id>.toml` (owners `ludovic111`, `source` pointing at the folder) and the file's
SHA-256. The reviewer builds it the same way and merges only when the hashes match: the `.wasm` holds no local paths,
so the same rustc and `Cargo.lock` give the same file.

| Mod | Id | Replaces | Settings (storage key: values) |
|---|---|---|---|
| FPS Counter | `fps_overlay` | Extras > FPS counter | `corner`: `top_left` (default), `top_right`, `bottom_left`, `bottom_right`; `lows`: `false` (default), `true` |
| Keystrokes | `keystrokes` | Extras > Keystrokes | `corner`: as above, default `top_left`; `cps`: `true` (default), `false` |
| Zoom | `zoom` | Extras > Zoom | `level`: `2`, `3`, `4` (default), `6`, `8`, `10`; `smooth`: `true` (default), `false`; `sensitivity` (slower turning): `true` (default), `false`. The key is the game's: `key.zoom.zoom` in `horizon-mods.txt`, default `key.keyboard.z` |
| Minimap | `minimap` | Extras > Minimap | `corner`: as above, default `top_right`; `size`: `64`, `96` (default), `128`; `shape`: `square` (default), `round`; `zoom` (blocks per pixel): `0.5`, `1` (default), `2`, `4`; `rotate`: `false` (default), `true`. The game draws the map (`render.set-minimap`) |
| Far View | `far_view` | Graphics > LOD Distance and Far Detail | `distance` (chunks): `64`, `128`, `256` (default), `512`, `1024`; `detail`: `full` (default), `reduced`. The game builds and draws the distant terrain (`render.set-far-terrain`); the player's LOD Distance, when on, wins |
| Recipes | `recipes` | nothing (new) | `give` (a click on the item panel gives the item in creative mode: one, a stack with a right click): `false` (default), `true`. The keys are the game's: `key.recipes.recipe` (default `key.keyboard.r`) and `key.recipes.uses` (default `key.keyboard.u`) in `horizon-mods.txt`. Every item sits in the item panel (`browser`) with a search box (words in the name or id, `@namespace`, `#tag`); R or a click shows an item's recipes, U or a right click its uses, a tab per station, and + fills the grid with a recipe the player knows (`recipes.place`). On servers that are not Horizon it shows the game's own 26.3 recipes; known recipes and + follow the server's recipe book |

## Versions and compatibility

- The API is the WIT package `horizon:mods@0.1.0`. New functions and
  interfaces arrive in patch versions (0.1.1, ...); a game serving 0.1.N loads
  mods built for 0.1.0 to 0.1.N. A breaking change makes `0.2` (and later
  `1.0`); the game then links both versions side by side for at least one
  release cycle, so existing mods keep working while authors move.
- The manifest's `api` names the line it needs (`0.1`); the game refuses a
  line it does not serve with a clear message.
- Within a line, each function has an API level, the one it arrived at
  ([stability](api/stability.md#mod-api-levels)). A mod needs the level of
  the newest function it calls, read from its file: an older Horizon refuses
  it with "made for a newer Horizon: update Horizon", and the registry lists
  the level so installers take a version the game can run.
  `cargo horizon-mod check` prints it ("needs mod API level 1 or newer").
- Horizon follows Minecraft releases; the mod API does not have to. Vanilla
  concepts mods see (dimensions, block and item ids) are the game's own ids,
  so a version port changes the data, not the API.

## Multiplayer

- **Client mods** (`side = "client"`, all of 0.1) run in the player's game,
  in single player and on any server, vanilla or Horizon. They see only what
  the client already knows (the snapshot), like F3 does, and act only on the
  local screen and chat. A server cannot see them.
- **Server mods** (`side = "server"`, API 0.3) run where the world is
  simulated: single player's server thread (or the window, in-frame), a LAN
  host and the dedicated server ([Server mods](#server-mods-api-03-experimental)).
  They use only vanilla messages, so any client, vanilla or Horizon, can join.
- **Both** (`side = "both"`): a server mod with a client half (new blocks,
  their textures, a custom screen). Nothing is announced or sent on join
  yet: a Horizon client needs the same mod on — its content loads from the
  same `.hmod` the server's did, so the ids match.
- Registries (new blocks and items) are not synced to clients: a vanilla
  client joins but does not see a server mod's blocks. Registry sync is a
  later stage.

## Distribution

- **The file.** `.hmod`: a zip holding `horizon-mod.toml`, `mod.wasm` and,
  later, `assets/` and `data/`. `mod.wasm` is capped at 32 MiB.
- **Integrity.** Every download is checked against the SHA-256 the index
  lists before the file reaches `horizon-mods/`.
- **Signing.** Authors sign `.hmod` files with an Ed25519 key (minisign
  format: a `.hmod.minisig` beside it); the index pins each project's public
  key, so an update signed by another key is refused. Unsigned local files
  stay allowed, with an "unsigned" mark on the Mods page.
- **Where mods come from.** Horizon's registry ([Publish your mod](#publish-your-mod)): reviewed pull requests in
  [horizon-rust/horizon-mods](https://github.com/horizon-rust/horizon-mods), files as its release assets, and an
  `index.json` on GitHub Pages that the launcher and the website read. A file the registry takes down
  (`revoked.toml`) is refused by the launcher and turned off where installed.
- **The launcher.** A Mods tab per installation: browse and search the index,
  install, update, turn on and off (it writes the same `horizon-mods.txt`),
  see capabilities before installing. The launcher's CLI and MCP tools gain
  `mods list|search|install|remove|enable|disable`, so agents can set up a
  modded game the same way players do.

## Roadmap

1. Design (this page) and the Java research.
2. SDK, example mod, runtime, `cargo horizon-mod`, the game's Mods page: API 0.1.
3. `storage` (done), `keys`, `target`, `inventory-read`, `ui`: enough for the
   first native ports (zoom presets, AppleSkin-style food HUD, Jade-style
   tooltips).
4. Launcher Mods tab, the index, signatures; the Mods page shows cost and
   update state.
5. `recipes`, `world-read`, `render`: recipe viewer and schematics ports.
6. Server mods, `registries`, `net`, data-driven content from `.hmod`
   `data/` and `assets/`, multiplayer sync.

Modpacks ([modpacks.md](modpacks.md)) move data-driven `registries` first:
API 0.2 is content-only `.hmod`s (blocks, items, recipes, loot, models and
textures), then server hooks (0.3) and block entities, menus, capabilities and
`net` (0.4), in the order the ATM10 inventory needs them.
