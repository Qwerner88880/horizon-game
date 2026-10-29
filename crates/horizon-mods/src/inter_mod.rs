//! The message bus between server mods (capability `inter-mod`,
//! docs/mods.md): what they exposed, the channels they subscribed and the
//! messages waiting for the tick's end. All in memory, nothing of it saved
//! with the world — a mod re-exposes and re-subscribes in its `init`, and
//! the bus dies with the run.
use crate::manifest::{Manifest, serves};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// APIs a mod may expose (`inter-mod.expose`).
pub const EXPOSES_PER_MOD: usize = 8;
/// Channels a mod may subscribe (`inter-mod.subscribe`).
pub const CHANNELS_PER_MOD: usize = 16;
/// Messages and bytes one hook call may put on the bus, whatever it sends
/// with (`publish`, `send-to`).
pub const MESSAGES_PER_CALL: u32 = 64;
pub const BYTES_PER_CALL: usize = 256 * 1024;
/// Messages and bytes one mod may be handed in a tick; more wait for the
/// next one, still in order.
pub const MESSAGES_PER_TICK: usize = 64;
pub const BYTES_PER_TICK: usize = 512 * 1024;
/// A mod's messages that may wait for it at once, and their bytes; new
/// ones past that are dropped (with a warning in the log).
pub const PENDING_PER_MOD: usize = 256;
pub const PENDING_BYTES_PER_MOD: usize = 512 * 1024;
/// Delivery rounds a tick: an answer a mod sends while hearing a message
/// is delivered by the next round, so a call chain of N hops needs N
/// rounds; what the last round still leaves waits for the next tick.
pub const ROUNDS_PER_TICK: usize = 4;

/// One message on the bus, as its sender left it.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    /// The sending mod's id.
    pub from: String,
    /// Who it is for: one mod (`send-to`) or a channel's subscribers
    /// (`publish`).
    pub target: Target,
    pub channel: String,
    pub data: Vec<u8>,
}

/// Where a message goes.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Mod(String),
    Channel,
}

/// One message due a mod, the target resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct Delivery {
    pub to: String,
    pub from: String,
    pub channel: String,
    pub data: Vec<u8>,
}

/// The bus the runtime keeps for the running server mods: exposed APIs,
/// subscriptions and the messages waiting for the tick's end. All of it is
/// plain state, no engine — the runtime calls into it from the mods' host
/// functions and hands the messages out at the tick's end.
#[derive(Default)]
pub struct InterMod {
    /// Each running mod's exposed APIs, as `expose` registered them
    /// (removed when the mod stops).
    exposed: BTreeMap<String, Vec<(String, String)>>,
    /// Each channel's subscribers, as `subscribe` registered them.
    subscribed: BTreeMap<String, BTreeSet<String>>,
    /// Messages waiting for this tick's rounds, in order.
    pending: VecDeque<Message>,
    /// Messages over a mod's tick caps: they wait for the next tick.
    next: VecDeque<Message>,
    /// What each mod has been handed this tick: (messages, bytes).
    delivered: BTreeMap<String, (usize, usize)>,
    /// The running mods that hear messages: they export
    /// `inter-mod-events` (a mod may claim the capability and still run
    /// code built without it — it is told nothing, and `send-to` to it is
    /// an error, so the promise "the other mod hears it" holds).
    pub hears: BTreeSet<String>,
}

impl InterMod {
    /// Registers mod `mod_id` as serving API `name` at `version`, for
    /// `providers`; a second `expose` of the same name moves its version.
    pub fn expose(&mut self, mod_id: &str, name: &str, version: &str) -> Result<(), String> {
        let list = self.exposed.entry(mod_id.to_owned()).or_default();
        if let Some((_, v)) = list.iter_mut().find(|(n, _)| n == name) {
            *v = version.to_owned();
            return Ok(());
        }
        if list.len() >= EXPOSES_PER_MOD {
            return Err(format!("at most {EXPOSES_PER_MOD} APIs a mod"));
        }
        list.push((name.to_owned(), version.to_owned()));
        Ok(())
    }

    /// The running mods serving API `name`: (id, version), by id.
    pub fn providers(&self, name: &str) -> Vec<(String, String)> {
        self.exposed
            .iter()
            .filter_map(|(id, list)| {
                list.iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, v)| (id.clone(), v.clone()))
            })
            .collect()
    }

    /// Subscribes mod `mod_id` to `channel` (its `publish` reaches it).
    pub fn subscribe(&mut self, mod_id: &str, channel: &str) -> Result<(), String> {
        let mine = self
            .subscribed
            .values()
            .filter(|list| list.contains(mod_id))
            .count();
        let list = self.subscribed.entry(channel.to_owned()).or_default();
        if mine >= CHANNELS_PER_MOD && !list.contains(mod_id) {
            return Err(format!("at most {CHANNELS_PER_MOD} channels a mod"));
        }
        list.insert(mod_id.to_owned());
        Ok(())
    }

    /// Queues `message` for the tick's end. False when a mod it is for
    /// already has more waiting than [`PENDING_PER_MOD`] allows: the
    /// message is dropped, and the caller says so in the log.
    pub fn push(&mut self, message: Message) -> bool {
        for to in self.recipients(&message) {
            let (n, bytes) = self.owed(&to);
            if n >= PENDING_PER_MOD || bytes + message.data.len() > PENDING_BYTES_PER_MOD {
                return false;
            }
        }
        self.pending.push_back(message);
        true
    }

    /// The mods `message` is for, its target resolved: a channel's
    /// subscribers that hear messages, never the sender itself.
    fn recipients(&self, message: &Message) -> Vec<String> {
        match &message.target {
            Target::Mod(id) => vec![id.clone()],
            Target::Channel => self
                .subscribed
                .get(&message.channel)
                .into_iter()
                .flatten()
                .filter(|id| **id != message.from && self.hears.contains(*id))
                .cloned()
                .collect(),
        }
    }

    /// (messages, bytes) waiting for `to`, the channels resolved.
    fn owed(&self, to: &str) -> (usize, usize) {
        self.pending
            .iter()
            .chain(&self.next)
            .filter(|m| self.recipients(m).contains(&to.to_owned()))
            .fold((0, 0), |(n, bytes), m| (n + 1, bytes + m.data.len()))
    }

    /// The tick's delivery begins: what waited over a mod's tick caps
    /// joins the queue, and every mod's tick caps start over.
    pub fn begin_tick(&mut self) {
        self.pending.append(&mut self.next);
        self.delivered.clear();
    }

    /// One round of the tick's delivery: the messages due now, each
    /// addressee resolved, within its tick caps. A message the caps keep
    /// back waits for the next tick; one whose addressee hears nothing —
    /// stopped, or the sender itself — is dropped. What a mod sends while
    /// hearing a message (after this returns) waits for the next round.
    pub fn take_round(&mut self) -> Vec<Delivery> {
        let mut out = Vec::new();
        let mut later = VecDeque::new();
        for m in std::mem::take(&mut self.pending) {
            for to in self.recipients(&m) {
                if to == m.from || !self.hears.contains(&to) {
                    continue;
                }
                let (n, bytes) = self.delivered.get(&to).copied().unwrap_or((0, 0));
                if n >= MESSAGES_PER_TICK || bytes + m.data.len() > BYTES_PER_TICK {
                    later.push_back(Message {
                        from: m.from.clone(),
                        target: Target::Mod(to.clone()),
                        channel: m.channel.clone(),
                        data: m.data.clone(),
                    });
                    continue;
                }
                self.delivered
                    .insert(to.clone(), (n + 1, bytes + m.data.len()));
                out.push(Delivery {
                    to,
                    from: m.from.clone(),
                    channel: m.channel.clone(),
                    data: m.data.clone(),
                });
            }
        }
        // Not back into `pending`: the kept-back messages wait for the next
        // tick, while what the delivered-to mods now send waits for the
        // next round of this one.
        self.next.extend(later);
        out
    }

    /// Takes a stopped mod off the bus: its exposes and subscriptions go,
    /// with the messages waiting for it and the ones it sent, and it hears
    /// nothing further.
    pub fn forget(&mut self, mod_id: &str) {
        self.exposed.remove(mod_id);
        self.subscribed.retain(|_, list| list.remove(mod_id));
        let gone = |m: &Message| m.from == mod_id || m.target == Target::Mod(mod_id.to_owned());
        self.pending.retain(|m| !gone(m));
        self.next.retain(|m| !gone(m));
        self.delivered.remove(mod_id);
        self.hears.remove(mod_id);
    }

    /// The bus holds nothing to deliver.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty() && self.next.is_empty()
    }
}

/// Where a game's mods start, by their `requires` (capability
/// `inter-mod`): the ones it starts, providers before the mods that need
/// them (a library's `init` exposes its API before the consumer asks for
/// it), else the order their paths came in; and each started mod's unmet
/// need, for its row's reason. `starts` says which mods this game starts
/// (`on`, and its side is this game's to run); `enabled` is the wider pool
/// a need may look in — a provider that runs elsewhere (a window's game,
/// for a server mod pair) still counts, since it runs where the consumer
/// will.
pub fn start_order(
    manifests: &[Option<&Manifest>],
    starts: &[bool],
    enabled: &[bool],
) -> (Vec<usize>, Vec<Option<String>>) {
    let served = |need: &str| {
        manifests.iter().enumerate().any(|(j, m)| {
            enabled[j] && m.is_some_and(|m| m.provides.iter().any(|p| serves(p, need)))
        })
    };
    let mut gate = vec![None; manifests.len()];
    for i in 0..manifests.len() {
        if !starts[i] {
            continue;
        }
        if let Some(need) = manifests[i].and_then(|m| m.requires.iter().find(|r| !served(r))) {
            gate[i] = Some(format!("needs {need}; no turned-on mod provides it"));
        }
    }
    // A started mod waits for each started provider of its needs; ones
    // that failed their own gate, or run elsewhere, wait for nothing.
    let running: Vec<bool> = (0..manifests.len())
        .map(|i| starts[i] && gate[i].is_none())
        .collect();
    let waits_for = |i: usize| {
        let mut deps = Vec::new();
        for need in manifests[i].map_or([].as_slice(), |m| &m.requires[..]) {
            deps.extend((0..manifests.len()).filter(|&j| {
                j != i
                    && running[j]
                    && manifests[j].is_some_and(|m| m.provides.iter().any(|p| serves(p, need)))
            }));
        }
        deps
    };
    // Kahn's order picking the smallest index among the ready, so no
    // dependencies is the paths' own order, as it always was.
    let mut placed = vec![false; manifests.len()];
    let mut order = Vec::new();
    while let Some(i) = (0..manifests.len())
        .find(|&i| running[i] && !placed[i] && waits_for(i).iter().all(|&j| placed[j]))
    {
        placed[i] = true;
        order.push(i);
    }
    // What never became ready needs what it needs in a circle.
    for i in 0..manifests.len() {
        if running[i] && !placed[i] {
            let need = manifests[i]
                .and_then(|m| m.requires.first())
                .map_or("", String::as_str);
            gate[i] = Some(format!("needs {need}; a circle of requires"));
        }
    }
    (order, gate)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bus where two mods hear messages, subscribed to nothing.
    fn a_bus(hearing: &[&str]) -> InterMod {
        let mut bus = InterMod::default();
        for id in hearing {
            bus.hears.insert((*id).to_owned());
        }
        bus
    }

    fn message(from: &str, target: Target, channel: &str, data: &[u8]) -> Message {
        Message {
            from: from.into(),
            target,
            channel: channel.into(),
            data: data.to_vec(),
        }
    }

    #[test]
    fn exposes_serve_providers_in_id_order() {
        let mut bus = a_bus(&[]);
        bus.expose("zeta", "spin-lib", "1.0").unwrap();
        bus.expose("alpha", "spin-lib", "2.1").unwrap();
        assert_eq!(
            bus.providers("spin-lib"),
            [
                ("alpha".into(), "2.1".into()),
                ("zeta".into(), "1.0".into())
            ]
        );
        assert!(bus.providers("other").is_empty());
        // A second expose of the same name moves its version; nine are too
        // many.
        bus.expose("alpha", "spin-lib", "2.2").unwrap();
        assert_eq!(bus.providers("spin-lib")[0], ("alpha".into(), "2.2".into()));
        for i in 0..7 {
            bus.expose("alpha", &format!("api{i}"), "1.0").unwrap();
        }
        assert!(bus.expose("alpha", "one-too-many", "1.0").is_err());
    }

    #[test]
    fn subscriptions_hold_sixteen_channels() {
        let mut bus = a_bus(&[]);
        for i in 0..CHANNELS_PER_MOD {
            bus.subscribe("a", &format!("chan/{i}")).unwrap();
        }
        // A channel twice stays one; a seventeenth is an error.
        bus.subscribe("a", "chan/0").unwrap();
        assert!(bus.subscribe("a", "one-too-many").is_err());
        assert!(bus.subscribe("b", "one-too-many").is_ok());
        assert_eq!(
            bus.recipients(&message("x", Target::Channel, "chan/3", b"")),
            Vec::<String>::new(),
            "a hears nothing yet"
        );
    }

    #[test]
    fn a_message_never_returns_to_its_sender() {
        let mut bus = a_bus(&["a"]);
        bus.subscribe("a", "spin/tick").unwrap();
        bus.push(message("a", Target::Channel, "spin/tick", b"x"));
        assert_eq!(bus.take_round(), []);
        // Nor does a direct send reach it, once delivered as one to itself.
        bus.push(message("b", Target::Mod("b".into()), "spin/tick", b"x"));
        assert_eq!(bus.take_round(), []);
        assert!(bus.is_empty());
    }

    #[test]
    fn a_round_delivers_in_order_and_never_twice() {
        let mut bus = a_bus(&["a", "b"]);
        bus.subscribe("a", "spin/tick").unwrap();
        bus.push(message("b", Target::Channel, "spin/tick", b"1"));
        bus.push(message("c", Target::Mod("a".into()), "spin/angle", b"22"));
        let round = bus.take_round();
        assert_eq!(
            round,
            [
                Delivery {
                    to: "a".into(),
                    from: "b".into(),
                    channel: "spin/tick".into(),
                    data: b"1".to_vec()
                },
                Delivery {
                    to: "a".into(),
                    from: "c".into(),
                    channel: "spin/angle".into(),
                    data: b"22".to_vec()
                },
            ],
            "in the order they were sent"
        );
        assert!(bus.is_empty(), "each message is delivered once");
        // A mod that hears nothing — stopped, or built without the export —
        // is skipped by a publish.
        bus.push(message("b", Target::Mod("gone".into()), "spin/tick", b"3"));
        assert_eq!(bus.take_round(), []);
    }

    #[test]
    fn answers_take_the_next_round_of_the_same_tick() {
        let mut bus = a_bus(&["a", "b"]);
        bus.subscribe("a", "ask").unwrap();
        bus.subscribe("b", "answer").unwrap();
        bus.push(message("b", Target::Channel, "ask", b"?"));
        let first = bus.take_round();
        assert_eq!(first.len(), 1);
        // The library, hearing it, answers: the answer rides the next
        // round of the same tick.
        bus.push(message("a", Target::Channel, "answer", b"!"));
        let second = bus.take_round();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].to, "b");
        assert_eq!(second[0].data, b"!");
        assert!(bus.is_empty());
    }

    #[test]
    fn a_flood_reaches_a_mod_in_ticks_of_sixty_four() {
        let mut bus = a_bus(&["a"]);
        for i in 0..MESSAGES_PER_TICK + 10 {
            bus.push(message(
                "b",
                Target::Mod("a".into()),
                "spin/tick",
                &[i as u8],
            ));
        }
        let first = bus.take_round();
        assert_eq!(first.len(), MESSAGES_PER_TICK);
        assert_eq!(first[0].data, [0], "the oldest go first");
        // Not the next round of the same tick — the kept-back messages
        // wait for the next tick, in order.
        assert_eq!(bus.take_round(), []);
        bus.begin_tick();
        let second = bus.take_round();
        assert_eq!(second.len(), 10);
        assert_eq!(second[0].data, [MESSAGES_PER_TICK as u8]);
        // Bytes cap the same way: 512 KiB across the tick's messages.
        let mut bus = a_bus(&["a"]);
        let big = vec![0u8; 32 * 1024];
        for _ in 0..MESSAGES_PER_TICK {
            bus.push(message("b", Target::Mod("a".into()), "spin/tick", &big));
        }
        assert_eq!(bus.take_round().len(), BYTES_PER_TICK / big.len());
    }

    #[test]
    fn a_mods_waiting_messages_are_capped_and_dropped() {
        let mut bus = a_bus(&["a"]);
        for i in 0..PENDING_PER_MOD {
            assert!(bus.push(message(
                "b",
                Target::Mod("a".into()),
                "spin/tick",
                &[i as u8],
            )));
        }
        assert!(
            !bus.push(message("b", Target::Mod("a".into()), "spin/tick", b"x")),
            "past 256 waiting for one mod, a message is dropped"
        );
        // The cap is per addressee: another mod's message still queues.
        assert!(bus.push(message("b", Target::Mod("c".into()), "spin/tick", b"x")));
        // However it is addressed, one more for the full mod goes too.
        assert!(!bus.push(message("c", Target::Mod("a".into()), "spin/tick", b"x")));
    }

    #[test]
    fn forgetting_a_mod_takes_it_and_its_messages_off_the_bus() {
        let mut bus = a_bus(&["a", "b", "c"]);
        bus.expose("a", "spin-lib", "1.0").unwrap();
        bus.subscribe("a", "ask").unwrap();
        bus.push(message("a", Target::Mod("b".into()), "spin/tick", b"mine"));
        bus.push(message(
            "b",
            Target::Mod("a".into()),
            "spin/tick",
            b"theirs",
        ));
        bus.push(message("b", Target::Mod("c".into()), "spin/tick", b"kept"));
        bus.forget("a");
        assert!(bus.providers("spin-lib").is_empty());
        assert!(
            !bus.recipients(&message("x", Target::Channel, "ask", b""))
                .contains(&"a".to_owned())
        );
        let round = bus.take_round();
        assert_eq!(round.len(), 1);
        assert_eq!(round[0].data, b"kept");
    }

    /// A manifest as `start_order` reads them, for the tests below.
    fn manifest(text: &str) -> Manifest {
        Manifest::parse(&format!(
            "id = \"m\"\nname = \"M\"\nversion = \"1\"\napi = \"0.3\"\nside = \"server\"\n\
             capabilities = [\"inter-mod\"]\nexperimental = [\"inter-mod\"]\n{text}"
        ))
        .unwrap()
    }

    #[test]
    fn without_needs_the_paths_own_order_stands() {
        let a = manifest("");
        let b = manifest("");
        let (order, gate) = start_order(&[Some(&a), Some(&b)], &[true, true], &[true, true]);
        assert_eq!(order, [0, 1]);
        assert_eq!(gate, [None, None]);
        // A mod this game doesn't start is neither ordered nor gated.
        let (order, gate) = start_order(&[Some(&a), Some(&b)], &[true, false], &[true, true]);
        assert_eq!(order, [0]);
        assert_eq!(gate, [None, None]);
    }

    #[test]
    fn providers_start_before_the_mods_that_need_them() {
        let lib = manifest("provides = [\"spin-lib@1.0\"]\n");
        let bell = manifest("requires = [\"spin-lib@1\"]\n");
        // The consumer's path comes first: it still starts second.
        let (order, gate) = start_order(&[Some(&bell), Some(&lib)], &[true, true], &[true, true]);
        assert_eq!(order, [1, 0]);
        assert_eq!(gate, [None, None]);
        // A provider that runs elsewhere (a window's game) needs no order
        // here, and its consumer's gate passes: both run where the world
        // does, its property.
        let (order, gate) = start_order(&[Some(&bell), Some(&lib)], &[true, false], &[true, true]);
        assert_eq!(order, [0]);
        assert_eq!(gate[0], None);
    }

    #[test]
    fn an_unmet_need_keeps_the_mod_off_with_its_reason() {
        let lib = manifest("provides = [\"spin-lib@1.0\"]\n");
        let bell = manifest("requires = [\"spin-lib@1\"]\n");
        // The library turned off: its file unread, or the player's choice.
        let (order, gate) = start_order(&[Some(&bell), Some(&lib)], &[true, false], &[true, false]);
        assert!(order.is_empty());
        assert_eq!(
            gate[0].as_deref(),
            Some("needs spin-lib@1; no turned-on mod provides it")
        );
        assert_eq!(gate[1], None, "a mod off is not gated, only not started");
        // A newer major is not a provider either.
        let v2 = manifest("provides = [\"spin-lib@2.0\"]\n");
        let (_, gate) = start_order(&[Some(&bell), Some(&v2)], &[true, true], &[true, true]);
        assert!(gate[0].is_some());
    }

    #[test]
    fn a_circle_of_needs_keeps_both_off() {
        let a = manifest("provides = [\"a@1.0\"]\nrequires = [\"b@1\"]\n");
        let b = manifest("provides = [\"b@1.0\"]\nrequires = [\"a@1\"]\n");
        let (order, gate) = start_order(&[Some(&a), Some(&b)], &[true, true], &[true, true]);
        assert!(order.is_empty());
        assert_eq!(gate[0].as_deref(), Some("needs b@1; a circle of requires"));
        assert_eq!(gate[1].as_deref(), Some("needs a@1; a circle of requires"));
    }
}
