//! Capability `machine-recipes` (docs/mods.md, "Machine recipes"): the
//! recipes of the server mods' own machines, read from their packages'
//! `data/<id>/recipe/` files and the content mods carrying them, before the
//! mods start — so a mod's `init` already finds its table.
use horizon_client::items::Items;
use horizon_mods::{
    Capability, Package, Runs, Side,
    machine_recipes::{self, MachineRecipes},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// The machine recipes of the server mods at `paths` (capability
/// `machine-recipes`): each one's own `data/<id>/recipe/` files, plus those
/// a content mod carries under the same id in `root`, the layered resource
/// root (the package's file wins on the same recipe id), tags expanded
/// through the item registry. None when no mod that runs here carries any:
/// the table is built once, in `Game::start_mods`, and never changes while
/// the mods run.
pub(super) fn machine_recipes(
    paths: &[PathBuf],
    root: &Path,
    items: Option<&Items>,
    runs: Runs,
) -> Option<Arc<MachineRecipes>> {
    if runs == Runs::Client {
        return None; // The server mods run in another game.
    }
    let mut files: Vec<(String, String)> = Vec::new();
    let mut any = false;
    for path in paths {
        let Ok(package) = Package::read(path) else {
            continue;
        };
        let m = &package.manifest;
        if m.side != Side::Server || !m.capabilities.contains(&Capability::MachineRecipes) {
            continue;
        }
        any = true;
        // The content layer first, the package last: a later file with the
        // same recipe id wins.
        files.extend(machine_recipes::files_in_dir(root, &m.id));
        files.extend(machine_recipes::files_in_package(path, &m.id));
    }
    if !any {
        return None;
    }
    let expand = |tag: &str| -> Vec<String> {
        items
            .and_then(|i| i.item_tag_order.get(tag))
            .cloned()
            .unwrap_or_default()
    };
    Some(Arc::new(machine_recipes::build(&files, &expand)))
}
