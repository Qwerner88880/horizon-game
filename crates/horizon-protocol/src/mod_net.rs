//! Mod messages: bytes a server mod and the client half of the same mod
//! (same id) send each other, on one custom payload channel in the play
//! state (docs/mods.md, capability `network`).
//!
//! The server announces it carries them with an empty [`CHANNEL`] payload in
//! the configuration phase (a vanilla client ignores it); a Horizon client
//! that hears it sends its mods' messages, and the server sends only to
//! clients that identified as Horizon. A payload is the mod's id, a channel
//! name of the mod's own, then the bytes.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
};

/// The custom payload channel, both ways.
pub const CHANNEL: &str = "horizon:mod_net";
/// Characters in a mod id at most.
pub const MAX_MOD_ID: usize = 64;
/// Characters in a message's channel name at most.
pub const MAX_NAME: usize = 64;
/// Bytes of a message's data at most: a serverbound payload may hold
/// `ServerboundCustomPayloadPacket.MAX_PAYLOAD_SIZE` (32767) in all.
pub const MAX_DATA: usize = 30_000;

/// A message as it travels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub mod_id: String,
    pub name: String,
    pub data: Vec<u8>,
}

/// A mod id: `[a-z0-9_]{1,64}`, as the manifest's.
pub fn id_ok(id: &str) -> bool {
    (1..=MAX_MOD_ID).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// A channel name: `[a-z0-9_./-]{1,64}`.
pub fn name_ok(name: &str) -> bool {
    (1..=MAX_NAME).contains(&name.len())
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'/' | b'-')
        })
}

/// A message's payload; an error for a name, id or size outside the caps.
pub fn encode(message: &Message) -> Result<Vec<u8>> {
    if !id_ok(&message.mod_id) {
        return Err(Error::new("mod net: bad mod id"));
    }
    if !name_ok(&message.name) {
        return Err(Error::new("mod net: bad channel name"));
    }
    if message.data.len() > MAX_DATA {
        return Err(Error::new("mod net: data too big"));
    }
    let mut w = Writer::new();
    w.string(&message.mod_id);
    w.string(&message.name);
    w.byte_array(&message.data);
    Ok(w.data)
}

/// A payload's message; an error for one that breaks a cap.
pub fn decode(data: &[u8]) -> Result<Message> {
    let mut r = Reader::new(data);
    let mod_id = r.string(MAX_MOD_ID)?;
    let name = r.string(MAX_NAME)?;
    let data = r.byte_array(MAX_DATA)?;
    if r.remaining() > 0 {
        return Err(Error::new("mod net: trailing bytes"));
    }
    if !id_ok(&mod_id) {
        return Err(Error::new("mod net: bad mod id"));
    }
    if !name_ok(&name) {
        return Err(Error::new("mod net: bad channel name"));
    }
    Ok(Message { mod_id, name, data })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(data: Vec<u8>) -> Message {
        Message {
            mod_id: "cells".into(),
            name: "cell/state".into(),
            data,
        }
    }

    #[test]
    fn a_message_round_trips() {
        for data in [vec![], vec![0, 255, 7], vec![9; MAX_DATA]] {
            let m = message(data);
            assert_eq!(decode(&encode(&m).unwrap()).unwrap(), m);
        }
    }

    #[test]
    fn caps_are_enforced_both_ways() {
        assert!(encode(&message(vec![0; MAX_DATA + 1])).is_err());
        let mut bad = message(vec![]);
        bad.name = "Has Spaces".into();
        assert!(encode(&bad).is_err());
        bad = message(vec![]);
        bad.mod_id = String::new();
        assert!(encode(&bad).is_err());
        // A payload built by hand with a name outside the rules.
        let mut w = Writer::new();
        w.string("cells");
        w.string("UPPER");
        w.byte_array(&[]);
        assert!(decode(&w.data).is_err());
        let mut ok = encode(&message(vec![1])).unwrap();
        ok.push(0);
        assert!(decode(&ok).is_err(), "trailing bytes");
        assert!(decode(&[]).is_err());
        assert!(decode(&[200, 200, 200]).is_err());
    }

    #[test]
    fn a_biggest_message_fits_a_serverbound_payload() {
        let m = Message {
            mod_id: "x".repeat(MAX_MOD_ID),
            name: "n".repeat(MAX_NAME),
            data: vec![0; MAX_DATA],
        };
        assert!(encode(&m).unwrap().len() <= 32767);
    }
}
