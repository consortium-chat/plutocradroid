#![feature(type_ascription)]
#[macro_use]
extern crate diesel;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate maplit;

mod schema;
mod view_schema;
mod damm;
mod iron_diesel;

mod bot;
mod web;
mod is_win;

use std::env;

fn main() {
    dotenv::dotenv().unwrap();

    if env::var_os("RUN_BOT") == Some("1".into()) {
        bot::bot_main();
    } else if env::var_os("RUN_WEBSERVER") == Some("1".into()) {
        web::web_main();
    } else {
        eprintln!("Must specify RUN_BOT=1 or RUN_WEBSERVER=1");
        std::process::exit(100);
    }
}