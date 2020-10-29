#![feature(type_ascription, proc_macro_hygiene, decl_macro, never_type)]

#[macro_use] extern crate log;
#[macro_use] extern crate rocket;
#[macro_use] extern crate diesel;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate maplit;
#[macro_use] extern crate serde;

mod models;
mod schema;
mod view_schema;
mod damm;
mod rocket_diesel;

mod bot;
mod web2;
mod is_win;

use std::env;

fn main() {
    dotenv::dotenv().unwrap();

    env_logger::init();
    if env::var_os("RUN_BOT") == Some("1".into()) {
        bot::bot_main();
    } else if env::var_os("RUN_WEB2") == Some("1".into()) {
        web2::main();
    } else {
        eprintln!("Must specify RUN_BOT=1 or RUN_WEB2=1");
        std::process::exit(100);
    }
}