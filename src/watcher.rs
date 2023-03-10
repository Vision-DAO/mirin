use actix_web::web::Data;
use notify::{
	event::ModifyKind, Config, EventKind, RecommendedWatcher, RecursiveMode,
	Result as NotifyResult, Watcher,
};
use serde::{Deserialize, Serialize};
use std::{
	collections::HashSet,
	fmt::Debug,
	fs::{canonicalize as fsabs, File},
	io::Read,
	path::{Path, PathBuf},
	process::{Command, Stdio},
	sync::{mpsc, Mutex},
};
use toml::{map::Map, Value};
use walkdir::WalkDir;

#[derive(Clone, Debug)]
pub struct Update {
	pub module: Vec<u8>,
	pub loader: Vec<u8>,
	pub nonce: usize,
}

#[derive(Serialize, Deserialize)]
struct CargoToml {
	dependencies: Map<String, Value>,
}

/// Recompiles the Beacon DAO, and the list of affected modules.
/// Recompiles all targets if no affected list is provided.
///
/// Returns None if compilation failed.
pub fn recompile(
	prev: Option<Update>,
	affected: Vec<impl AsRef<Path> + Debug>,
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
		.filter_map(|mod_name: String| mod_name.split("-").last().map(|part| part.to_owned()))
		.collect::<HashSet<String>>();

	// Find all members of beacon_dao that look like modules
	let all_modules = || {
		WalkDir::new(&dir)
			.max_depth(1)
			.into_iter()
			.filter_map(Result::ok)
			// Get only modules that were altered, and that are modules
			.filter_map(|ent| {
				let fname = ent.file_name().to_str()?;
				let mod_name = fname.split("-").last()?.to_owned();

				if fname.starts_with("beacon_dao-") {
					Some((ent, mod_name))
				} else {
					None
				}
			})
	};

	// Include modules that have dependencies on any of the included modules
	let dependents = all_modules()
		.filter_map(|(ent, mod_name)| {
			let mut cargo_path: PathBuf = PathBuf::from(ent.path().clone());
			cargo_path.push("Cargo.toml");

			// Find the dependencies in the Cargo.toml
			let mut buf = String::new();
			File::open(cargo_path).ok()?.read_to_string(&mut buf).ok()?;

			let conf: CargoToml = toml::from_str(buf.as_str()).ok()?;

			// The module depends on one of the modified modules
			// TODO: Make this recursive
			if conf
				.dependencies
				.keys()
				.filter_map(|k| Some(k.split("-").last()?.to_owned()))
				.collect::<HashSet<String>>()
				.intersection(&included)
				.next()
				.is_some()
			{
				return Some(mod_name);
			}

			None
		})
		.collect::<HashSet<String>>();
	let included = included
		.union(&dependents)
		.cloned()
		.collect::<HashSet<String>>();

	// Get only modules that were altered
	let all_targets = all_modules()
		.filter(|(_, mod_name)| included.contains(mod_name) || included.is_empty())
		.collect::<Vec<_>>();

	if all_targets.is_empty() {
		return prev;
	}

	println!("\nrecompiling {} targets", all_targets.len());

	for (target, _) in &all_targets {
		Command::new("cargo")
			.args([
				"build",
				"--target",
				"wasm32-unknown-unknown",
				"--release",
				"--features",
				"module",
			])
			.current_dir(target.path())
			.stderr(Stdio::inherit())
			.stdout(Stdio::inherit())
			.output()
			.expect("Failed to compile module");
	}

	// Always compile scheduler
	Command::new("cargo")
		.args(["make", "build_scheduler"])
		.current_dir(&dir)
		.stderr(Stdio::inherit())
		.stdout(Stdio::inherit())
		.output()
		.expect("Failed to compile scheduler");

	// Read the compiled module and JS loader
	let mut base = PathBuf::from(dir.as_ref());
	base.push("beacon_dao-scheduler/pkg");

	// Read the module source code
	let mut mod_buf = Vec::new();
	File::open({
		let mut mpath = base.clone();
		mpath.push("beacon_dao_scheduler_bg.wasm");

		mpath
	})
	.ok()?
	.read_to_end(&mut mod_buf)
	.ok()?;

	// Read the loader source code
	let mut loader_buf = Vec::new();
	File::open({
		let mut lpath = base.clone();
		lpath.push("beacon_dao_scheduler.js");

		lpath
	})
	.ok()?
	.read_to_end(&mut loader_buf)
	.ok()?;

	println!("\ncompiled {} targets!", all_targets.len());

	Some(Update {
		module: mod_buf,
		loader: loader_buf,
		nonce: prev.map(|update| update.nonce).unwrap_or(0) + 1,
	})
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
			EventKind::Create(_) | EventKind::Remove(_) => Some(e),
			EventKind::Modify(ref m) => match m {
				ModifyKind::Data(_) => Some(e),
				_ => None,
			},
			_ => None,
		})
		// Recompile all affected modules, and scheduler
		.map(|e| {
			let dir = fsabs(&dir).ok()?;

			fn is_terminal(pat: &Path, base_dir: &Path) -> Option<bool> {
				Some(fsabs(pat.parent()?).ok()? == fsabs(base_dir).ok()?)
			}

			fn terminal<'a>(pat: &'a Path, base_dir: &'a Path) -> Option<&'a Path> {
				if is_terminal(pat, base_dir).unwrap_or(false) {
					Some(pat)
				} else {
					terminal(pat.parent()?, base_dir)
				}
			}

			// Checks that the path is a change under the /src directory,
			// and that it is not outside the base dir
			fn is_src(pat: &Path, base_dir: &Path) -> Option<bool> {
				if !pat.starts_with(base_dir) {
					return Some(false);
				}

				if pat.parent()?.file_name()? == "src" {
					return Some(true);
				}

				is_src(pat.parent()?, base_dir)
			}

			// Convert each affected path to an absolute path
			let paths = e
				.paths
				.iter()
				.filter_map(|pat| fsabs(pat).ok())
				.collect::<Vec<PathBuf>>();

			let e = paths
				.iter()
				.filter_map(|pat| {
					if !is_src(pat.as_path(), dir.as_ref())? {
						None
					} else {
						terminal(&pat, dir.as_ref())
					}
				})
				.filter(|pat| !pat.ends_with("target"))
				.collect::<Vec<&Path>>();

			if e.is_empty() {
				return None;
			}

			let new_state = recompile(mod_buff.lock().unwrap().clone(), e, &dir);
			(*mod_buff.lock().unwrap()) = new_state;

			Some(())
		})
		.for_each(drop);

	Ok(())
}
