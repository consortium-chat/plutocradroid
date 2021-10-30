#![feature(type_ascription, proc_macro_hygiene, decl_macro, never_type)]

#[macro_use] extern crate log;
#[macro_use] extern crate rocket;
#[macro_use] extern crate diesel;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate maplit;
#[macro_use] extern crate serde;

#[macro_use] mod statics;

mod models;
#[allow(unused_imports)]
mod schema;
mod view_schema;
mod damm;
mod rocket_diesel;
mod bot;
mod web2;
mod is_win;
mod static_responders;
mod worker;
mod tasks;

use std::{env,panic,process};

#[cfg(feature = "debug")]
pub const SITE_URL:&str = "https://pluto-test.shelvacu.com";
#[cfg(not(feature = "debug"))]
pub const SITE_URL:&str = "https://mas.consortium.chat";

#[cfg(feature = "debug")]
lazy_static! {
    pub static ref GENERATE_EVERY:chrono::Duration = chrono::Duration::seconds(30);
    pub static ref MOTION_EXPIRATION:chrono::Duration = chrono::Duration::minutes(20);
    pub static ref AUCTION_EXPIRATION:chrono::Duration = chrono::Duration::minutes(20);
    pub static ref AUTO_AUCTION_AT:chrono::NaiveTime = chrono::NaiveTime::from_hms(23, 34, 45);
    pub static ref AUTO_AUCTION_EVERY:chrono::Duration = chrono::Duration::days(1);
}

#[cfg(not(feature = "debug"))]
lazy_static! {
    pub static ref GENERATE_EVERY:chrono::Duration = chrono::Duration::hours(24);
    pub static ref MOTION_EXPIRATION:chrono::Duration = chrono::Duration::hours(48);
    pub static ref AUCTION_EXPIRATION:chrono::Duration = chrono::Duration::hours(48);
    pub static ref AUTO_AUCTION_AT:chrono::NaiveTime = chrono::NaiveTime::from_hms(7,0,0);
    pub static ref AUTO_AUCTION_EVERY:chrono::Duration = chrono::Duration::days(7);
}

fn main() {
    lazy_static::initialize(&GENERATE_EVERY);
    lazy_static::initialize(&MOTION_EXPIRATION);
    lazy_static::initialize(&AUCTION_EXPIRATION);
    lazy_static::initialize(&AUTO_AUCTION_AT);
    lazy_static::initialize(&AUTO_AUCTION_EVERY);
    dotenv::dotenv().unwrap();

    //Die on error
    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(1);
    }));

    env_logger::init();
    if env::var_os("RUN_BOT") == Some("1".into()) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(bot::bot_main());
    } else if env::var_os("RUN_WEB2") == Some("1".into()) {
        web2::main();
    } else if env::var_os("RUN_WORKER") == Some("1".into()) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(worker::main());
    } else {
        eprintln!("Must specify RUN_BOT=1, RUN_WEB2=1, or RUN_WORKER=1");
        std::process::exit(100);
    }
}