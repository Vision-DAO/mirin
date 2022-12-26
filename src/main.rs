use actix_web::{web::Data, App, HttpServer};
use console::Term;
use mirin::{
	server::{checksum, index, loader, module},
	watcher::{recompile, watcher},
};
use std::{env, io, sync::Mutex, thread};

#[actix_web::main]
async fn main() -> io::Result<()> {
	env_logger::init();

	let dao_path = env::args().skip(1).next().expect("Missing beacon DAO path");
	let mod_buff = Data::new(Mutex::new(recompile(None, <Vec<&str>>::new(), &dao_path)));
	let term = Term::stdout();

	// Rebuild every time a file changes
	{
		let mod_buff = mod_buff.clone();
		let dao_path = dao_path.to_owned();

		thread::spawn(|| watcher(dao_path, mod_buff));
	}

	// Rebuild every time R gets pushed
	{
		let mod_buff = mod_buff.clone();

		thread::spawn(move || loop {
			// Check for the letter R
			let buffer = term.read_char().unwrap();

			// Trigger recompilation
			if buffer == 'R' {
				let new = recompile(
					mod_buff.lock().unwrap().clone(),
					<Vec<&str>>::new(),
					&dao_path,
				);

				(*mod_buff.lock().unwrap()) = new;
			}
		});
	}

	println!("listening on http://0.0.0.0:3000");

	HttpServer::new(move || {
		App::new()
			.app_data(Data::clone(&mod_buff))
			.service(index)
			.service(checksum)
			.service(loader)
			.service(module)
	})
	.bind(("0.0.0.0", 3000))?
	.run()
	.await
}
