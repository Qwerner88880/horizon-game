//! Example Horizon server mod (`docs/mods.md`, "Library mods and messages
//! between mods") with capability `inter-mod`, the consumer half of the
//! `spin-lib` pair: its manifest `requires` `spin-lib@1`, so the game
//! starts the library first and keeps this mod off without it. Each tick
//! it publishes its counter on `spin/tick`; the library answers on
//! `spin/angle`, and the first answer is broadcast. `/spin-self` sends to
//! the mod's own id, which the API refuses — the error is the feedback.
//!
//! Its test script runs only with the library beside it
//! (`TestGame::load_pair` in the sandbox battery); alone,
//! `cargo horizon-mod test` refuses the mod with "needs spin-lib@1", the
//! version gate doing its work.
use horizon_mod_sdk::server::{Args, Player, Registration, ServerMod, inter_mod, players};

struct SpinBell {
    /// One a tick, from a quarter turn: the first answer is 90°.
    counter: u64,
    /// The first answer was broadcast; the rest pass in silence.
    told: bool,
}

impl ServerMod for SpinBell {
    fn init() -> Result<(Self, Registration), String> {
        inter_mod::subscribe("spin/angle")?;
        // Who serves the need: read back from the bus, so a broken load
        // order shows in the broadcast, not in a silent tick.
        match inter_mod::providers("spin-lib").first() {
            Some(p) => players::broadcast(&format!("spin-lib {} from {}", p.version, p.mod_id)),
            None => players::broadcast("no spin-lib provider"),
        }
        Ok((
            SpinBell {
                counter: 90,
                told: false,
            },
            Registration::new().on_tick().command("spin-self"),
        ))
    }

    fn tick(&mut self, _tick: u64) {
        let _ = inter_mod::publish("spin/tick", &self.counter.to_le_bytes());
        self.counter += 1;
    }

    fn message(&mut self, _from: &str, channel: &str, data: &[u8]) {
        if channel != "spin/angle" || self.told || data.len() < 4 {
            return;
        }
        self.told = true;
        let angle = f32::from_le_bytes(data[..4].try_into().expect("4 bytes"));
        players::broadcast(&format!("spin at {angle}\u{b0}"));
    }

    fn command(
        &mut self,
        _player: &Player,
        name: &str,
        _args: &Args,
    ) -> Result<Option<String>, String> {
        if name != "spin-self" {
            return Err("This command does nothing".into());
        }
        inter_mod::send_to("spin_bell", "spin/self", b"hello?").map(|_| None)
    }
}

horizon_mod_sdk::export_server_mod!(SpinBell);
