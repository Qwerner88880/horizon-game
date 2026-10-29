//! Example Horizon client mod (`docs/mods.md`, "Network"): the client half of
//! the `cells` server mod, with the same id. The server's `/cellui` sends
//! `cells/open` and then `cells/state` messages (`x y z stored capacity in
//! out`, as text); this mod draws them as a screen with the mod's widgets —
//! the charge, a bar, the redstone in and out, and two buttons — and sends
//! `cells/action` (`fill`, `drain`, `close`) back for what the player presses.
//! The server decides what each does; this half only shows and asks.
use horizon_mod_sdk::{
    Mod, Registration, network,
    ui::{self, ButtonWidget, ItemWidget, Screen, TextWidget, Widget},
};

/// The screen's widgets by id: the buttons the player presses.
const FILL: u32 = 1;
const DRAIN: u32 = 2;

/// What the server last said of the cell.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Cell {
    x: i32,
    y: i32,
    z: i32,
    stored: u64,
    capacity: u64,
    input: u32,
    output: u32,
}

/// `x y z stored capacity in out`; `None` for anything else.
fn parse(data: &[u8]) -> Option<Cell> {
    let text = std::str::from_utf8(data).ok()?;
    let mut f = text.split(' ');
    let cell = Cell {
        x: f.next()?.parse().ok()?,
        y: f.next()?.parse().ok()?,
        z: f.next()?.parse().ok()?,
        stored: f.next()?.parse().ok()?,
        capacity: f.next()?.parse().ok()?,
        input: f.next()?.parse().ok()?,
        output: f.next()?.parse().ok()?,
    };
    (f.next().is_none() && cell.capacity > 0).then_some(cell)
}

/// A bar of ten cells and the percentage: `[|||||.....] 50%`.
fn bar(stored: u64, capacity: u64) -> String {
    let stored = stored.min(capacity);
    let filled = (stored * 10 / capacity) as usize;
    format!(
        "[{}{}] {}%",
        "|".repeat(filled),
        ".".repeat(10 - filled),
        stored * 100 / capacity
    )
}

/// The screen for a cell.
fn screen(c: &Cell) -> Screen {
    let text = |id: u32, text: String, color: u32| Widget::Text(TextWidget { id, text, color });
    Screen {
        title: format!("Energy Cell {} {} {}", c.x, c.y, c.z),
        widgets: vec![
            text(10, format!("{} / {} FE", c.stored, c.capacity), 0xFFAA00),
            text(11, bar(c.stored, c.capacity), 0xFF5555),
            Widget::Item(ItemWidget {
                id: 12,
                item: "minecraft:redstone".into(),
                name: format!("{} waiting, {} made", c.input, c.output),
            }),
            Widget::Button(ButtonWidget {
                id: FILL,
                label: "Fill 1000 FE".into(),
            }),
            Widget::Button(ButtonWidget {
                id: DRAIN,
                label: "Drain 1000 FE into redstone".into(),
            }),
        ],
    }
}

#[derive(Default)]
struct CellsUi {
    /// The cell on the screen, while one is open.
    cell: Option<Cell>,
}

impl Mod for CellsUi {
    fn init() -> Result<(Self, Registration), String> {
        Ok((CellsUi::default(), Registration::new()))
    }

    fn network_received(&mut self, channel: &str, data: &[u8]) {
        match channel {
            // The server opened a screen, or its state changed: a second
            // `open-screen` while one shows replaces it, so one call serves
            // both, with the new numbers.
            "cells/open" | "cells/state" => {
                if channel == "cells/state" && self.cell.is_none() {
                    // A late update after the player left the screen.
                    return;
                }
                let Some(cell) = parse(data) else { return };
                ui::open_screen(&screen(&cell));
                self.cell = Some(cell);
            }
            // The cell the screen showed is gone.
            "cells/gone" => {
                self.cell = None;
                ui::close_screen();
            }
            _ => {}
        }
    }

    /// A button: asks the server, which answers with the new state.
    fn ui_select(&mut self, id: u32, _row: u32) {
        let action: &[u8] = match id {
            FILL => b"fill",
            DRAIN => b"drain",
            _ => return,
        };
        let _ = network::send("cells/action", action);
    }

    /// Esc or Done: the server stops sending states.
    fn ui_closed(&mut self) {
        self.cell = None;
        let _ = network::send("cells/action", b"close");
    }
}

horizon_mod_sdk::export_mod!(CellsUi);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_parse_and_nothing_else_does() {
        let c = parse(b"-38 64 -8 5000 100000 2 3").unwrap();
        assert_eq!(
            (c.x, c.y, c.z, c.stored, c.input, c.output),
            (-38, 64, -8, 5000, 2, 3)
        );
        for bad in [
            "",
            "1 2 3",
            "1 2 3 4 0 0 0",
            "1 2 3 4 5 6 7 8",
            "a b c d e f g",
        ] {
            assert_eq!(parse(bad.as_bytes()), None, "{bad:?}");
        }
        assert_eq!(parse(&[0xFF, 0xFE]), None);
    }

    #[test]
    fn the_bar_fills_by_tenths() {
        assert_eq!(bar(0, 100), "[..........] 0%");
        assert_eq!(bar(50, 100), "[|||||.....] 50%");
        assert_eq!(bar(100, 100), "[||||||||||] 100%");
        assert_eq!(
            bar(500, 100),
            "[||||||||||] 100%",
            "over capacity stays full"
        );
    }

    #[test]
    fn the_screen_has_two_buttons_and_the_numbers() {
        let c = parse(b"0 64 0 25000 100000 1 0").unwrap();
        let s = screen(&c);
        assert_eq!(s.title, "Energy Cell 0 64 0");
        let buttons = s
            .widgets
            .iter()
            .filter(|w| matches!(w, Widget::Button(_)))
            .count();
        assert_eq!(buttons, 2);
        assert!(matches!(&s.widgets[0], Widget::Text(t) if t.text == "25000 / 100000 FE"));
    }
}
