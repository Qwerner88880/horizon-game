//! Example Horizon server mod (`docs/mods.md`, "Library mods and messages
//! between mods") with capability `inter-mod`, the library half of the
//! `spin-bell` pair: exposes API `spin-lib` at `1.0` (as its manifest
//! `provides`), hears `spin/tick` — a `u64` counter, little-endian — and
//! answers its sender on `spin/angle` with the angle the counter stands
//! for, an `f32` in degrees. `/spin-flood` publishes 65 messages in one
//! call, to show the per-call cap in the feedback.
use horizon_mod_sdk::server::{Args, Player, Registration, ServerMod, inter_mod, players};

/// The angle a counter stands for, in degrees: one a tick, a turn every 360.
fn angle_of(counter: u64) -> f32 {
    (counter % 360) as f32
}

struct SpinLib;

impl ServerMod for SpinLib {
    fn init() -> Result<(Self, Registration), String> {
        inter_mod::expose("spin-lib", "1.0")?;
        inter_mod::subscribe("spin/tick")?;
        // Asked back over the same bus: the mod's own entry, as the mods
        // that need it will read it.
        let version = inter_mod::providers("spin-lib")
            .first()
            .map(|p| p.version.clone())
            .unwrap_or_default();
        players::broadcast(&format!("spin-lib {version} on"));
        Ok((SpinLib, Registration::new().command("spin-flood")))
    }

    fn message(&mut self, from: &str, channel: &str, data: &[u8]) {
        if channel != "spin/tick" || data.len() < 8 {
            return;
        }
        let counter = u64::from_le_bytes(data[..8].try_into().expect("8 bytes"));
        let _ = inter_mod::send_to(from, "spin/angle", &angle_of(counter).to_le_bytes());
    }

    fn command(
        &mut self,
        _player: &Player,
        name: &str,
        _args: &Args,
    ) -> Result<Option<String>, String> {
        if name != "spin-flood" {
            return Err("This command does nothing".into());
        }
        let (mut sent, mut refused) = (0, 0);
        for _ in 0..65 {
            match inter_mod::publish("spin/tick", &[]) {
                Ok(()) => sent += 1,
                Err(_) => refused += 1,
            }
        }
        Ok(Some(format!("sent {sent}, {refused} refused")))
    }
}

horizon_mod_sdk::export_server_mod!(SpinLib);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_become_angles_within_one_turn() {
        assert_eq!(angle_of(0), 0.0);
        assert_eq!(angle_of(90), 90.0);
        assert_eq!(angle_of(359), 359.0);
        assert_eq!(angle_of(360), 0.0);
        assert_eq!(angle_of(361), 1.0);
        assert_eq!(angle_of(u64::MAX), 15.0);
    }
}
