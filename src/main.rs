#[macro_use]
extern crate log;

use actix_web::{get, web::Data, App, HttpServer, Responder};
use mirin::{
	server::{checksum, index, loader, module},
	watcher::{recompile, watcher, Update},
};
use std::{env, io, sync::Mutex, thread};

#[actix_web::main]
async fn main() -> io::Result<()> {
	env_logger::init();

	let dao_path = env::args().skip(1).next().expect("Missing beacon DAO path");
	let mod_buff = Data::new(Mutex::new(recompile(None, vec![], dao_path)));

	thread::spawn(|| watcher(dao_path, mod_buff));

	HttpServer::new(|| {
		App::new()
			.service(index)
			.service(checksum)
			.service(loader)
			.service(module)
	})
	.bind(("0.0.0.0", 3000))?
	.run()
	.await
}
