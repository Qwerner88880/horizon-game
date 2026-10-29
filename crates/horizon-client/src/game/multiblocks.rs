//! Capability `multiblocks` (docs/mods.md, "Multiblocks"): the structures
//! of the server mods' own multiblocks, read from their packages'
//! `data/<id>/multiblock/` files and the content mods carrying them, before
//! the mods start — so a mod's `init` already finds its templates.
use horizon_mods::{
    Capability, Package, Runs, Side,
    multiblocks::{self, Templates},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// The structures of the server mods at `paths` (capability
/// `multiblocks`): each one's own `data/<id>/multiblock/` files, plus those
/// a content mod carries under the same id in `root`, the layered resource
/// root (the package's file wins on the same structure id). None when no
/// mod that runs here carries any: the table is built once, in
/// `Game::start_mods`, and never changes while the mods run.
pub(super) fn multiblocks(paths: &[PathBuf], root: &Path, runs: Runs) -> Option<Arc<Templates>> {
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
        if m.side != Side::Server || !m.capabilities.contains(&Capability::Multiblocks) {
            continue;
        }
        any = true;
        // The content layer first, the package last: a later file with the
        // same structure id wins.
        files.extend(multiblocks::files_in_dir(root, &m.id));
        files.extend(multiblocks::files_in_package(path, &m.id));
    }
    if !any {
        return None;
    }
    Some(Arc::new(multiblocks::build(&files)))
}
