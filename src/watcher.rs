use actix_web::web::Data;
use notify::{
	Config, EventKind, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use std::{
	collections::HashSet,
	path::Path,
	process::Command,
	sync::{mpsc, Mutex},
};
use walkdir::{DirEntry, WalkDir};

#[derive(Clone)]
pub struct Update {
	pub module: Vec<u8>,
	pub loader: Vec<u8>,
	pub nonce: usize,
}

/// Recompiles the Beacon DAO, and the list of affected modules.
/// Recompiles all targets if no affected list is provided.
///
/// Returns the previous state if compilation failed.
pub fn recompile(
	prev: Option<Update>,
	affected: Vec<impl AsRef<Path>>,
	dir: impl AsRef<Path>,
) -> Option<Update> {
	// Get the beacon-dao_PART part of each excluded module
	let included = affected
		.into_iter()
		.filter_map(|path| {
			path.as_ref()
				.file_name()
				.and_then(|fname| Some(fname.to_str()?.to_owned()))
		})
		.filter_map(|mod_name: String| mod_name.split("_").last().map(|part| part.to_owned()))
		.collect::<HashSet<String>>();

	// Find all members of beacon_dao that look like modules
	let all_modules = WalkDir::new(dir)
		.into_iter()
		.filter_map(Result::ok)
		// Get only modules that were altered, and that are modules
		.filter_map(|ent| {
			let fname = ent.file_name().to_str()?;
			let mod_name = fname.split("_").last()?;

			if fname.starts_with("beacon_dao-") {
				Some(ent)
			} else {
				None
			}
		});

	// Include modules that have dependencies on any of the included modules

	let all_targets = WalkDir::new(dir)
		.into_iter()
		.filter_map(Result::ok)
		// Get only modules that were altered, and that are modules
		.filter_map(|ent| {
			let fname = ent.file_name().to_str()?;
			let mod_name = fname.split("_").last()?;

			if fname.starts_with("beacon_dao-")
				&& (included.contains(mod_name) || included.is_empty())
			{
				Some(ent)
			} else {
				None
			}
		})
		.collect::<Vec<DirEntry>>();

	info!("recompiling {} targets", all_targets.len());

	for target in all_targets {
		Command::new("cargo")
			.args([
				"build",
				"--target",
				"wasm32-unknown-unknkown",
				"--release",
				"--features",
				"module",
			])
			.current_dir(target.path)
	}

	// Always compile scheduler

	prev
}

/// Waits for changes to modules, rebuilding the required parts of the beacon DAO.
pub fn watcher(dir: impl AsRef<Path>, mod_buff: Data<Mutex<Option<Update>>>) -> NotifyResult<()> {
	let (tx, rx) = mpsc::channel();

	// Every time the beacon DAO changes, rebuild the scheduler, and whichever modules were changed
	let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
	watcher.watch(dir.as_ref(), RecursiveMode::Recursive)?;

	rx.iter()
		.filter_map(Result::ok)
		// Deal only with events that can mutate the beacon DAO wasm
		.filter_map(|e| match e.kind {
			EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => Some(e),
			_ => None,
		})
		// Recompile all affected modules, and scheduler
		.for_each(|e| {
			let new_state = recompile(mod_buff.lock().unwrap().clone(), e.paths, dir);
			(*mod_buff.lock().unwrap()) = new_state;
		});

	Ok(())
}
