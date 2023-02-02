use crate::watcher::Update;
use actix_web::{get, web::Data, HttpResponse, Responder};
use std::sync::Mutex;

#[get("/")]
pub async fn index() -> impl Responder {
	HttpResponse::Ok()
		.content_type("text/html")
		.body(include_str!("../static/index.html"))
}

#[get("/module")]
pub async fn module(data: Data<Mutex<Option<Update>>>) -> impl Responder {
	if let Some(module) = data
		.lock()
		.ok()
		.as_ref()
		.and_then(|update| update.as_ref().map(|update| update.module.clone()))
	{
		HttpResponse::Ok()
			.content_type("application/json")
			.json(module)
	} else {
		HttpResponse::NotFound().body("Not Found")
	}
}

#[get("/loader")]
pub async fn loader(data: Data<Mutex<Option<Update>>>) -> impl Responder {
	if let Some(loader) = data
		.lock()
		.ok()
		.as_ref()
		.and_then(|update| update.as_ref().map(|update| update.loader.clone()))
	{
		HttpResponse::Ok()
			.content_type("application/json")
			.json(loader)
	} else {
		HttpResponse::NotFound().body("Not Found")
	}
}

#[get("/checksum")]
pub async fn checksum(data: Data<Mutex<Option<Update>>>) -> impl Responder {
	data.lock()
		.ok()
		.and_then(|lock| lock.as_ref().map(|update| update.nonce))
		.unwrap_or(0)
		.to_string()
}
