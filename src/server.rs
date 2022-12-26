use crate::watcher::Update;
use actix_web::{get, web::Data, Responder};
use std::sync::Mutex;

#[get("/")]
pub async fn index() -> impl Responder {
	include_str!("../static/index.html")
}

#[get("/module")]
pub async fn module(data: Data<Mutex<Option<Update>>>) -> Option<impl Responder> {
	data.lock()
		.ok()?
		.as_ref()
		.map(|update| update.module.clone())
}

#[get("/loader")]
pub async fn loader(data: Data<Mutex<Option<Update>>>) -> Option<impl Responder> {
	data.lock()
		.ok()?
		.as_ref()
		.map(|update| update.loader.clone())
}

#[get("/checksum")]
pub async fn checksum(data: Data<Mutex<Option<Update>>>) -> impl Responder {
	data.lock()
		.ok()
		.and_then(|lock| lock.as_ref().map(|update| update.nonce))
		.unwrap_or(0)
		.to_string()
}
