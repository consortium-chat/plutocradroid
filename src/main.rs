#![feature(type_ascription, proc_macro_hygiene, decl_macro, never_type)]

#[macro_use] extern crate log;
#[macro_use] extern crate rocket;
#[macro_use] extern crate diesel;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate maplit;
#[macro_use] extern crate serde;


mod models;
#[allow(unused_imports)]
mod schema;
mod view_schema;
mod damm;
mod bot;
mod is_win;
mod worker;
mod tasks;
mod fix_transactions;
mod transfers;
mod web;
mod names;

use std::{env,panic,process};

#[cfg(feature = "debug")]
pub const SITE_URL:&str = "https://pluto-test.shelvacu.com";
#[cfg(not(feature = "debug"))]
pub const SITE_URL:&str = "https://mas.consortium.chat";

// https://mas.consortium.chat/motions/2860
const MAX_MOTIONS_PER_DAY:u8 = 10;
// https://mas.consortium.chat/motions/2121
const MAX_MOTION_LENGTH_CODEPOINTS:u16 = 1000;

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
    pub static ref MOTION_EXPIRATION:chrono::Duration = chrono::Duration::hours(96);
    pub static ref AUCTION_EXPIRATION:chrono::Duration = chrono::Duration::hours(48);
    pub static ref AUTO_AUCTION_AT:chrono::NaiveTime = chrono::NaiveTime::from_hms(7,0,0);
    pub static ref AUTO_AUCTION_EVERY:chrono::Duration = chrono::Duration::days(7);
}

lazy_static! {
    pub static ref GIVE_DESTINATION_RE:regex::Regex = regex::Regex::new(r"^(?:([^\-\s]+)\s*-\s*)?(\d+)$").unwrap();
}

fn main() {
    lazy_static::initialize(&GENERATE_EVERY);
    lazy_static::initialize(&MOTION_EXPIRATION);
    lazy_static::initialize(&AUCTION_EXPIRATION);
    lazy_static::initialize(&AUTO_AUCTION_AT);
    lazy_static::initialize(&AUTO_AUCTION_EVERY);
    lazy_static::initialize(&GIVE_DESTINATION_RE);
    dotenv::dotenv().unwrap();

    //Die on error
    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // invoke the default handler and exit the process
        orig_hook(panic_info);
        process::exit(1);
    }));

    env_logger::init();
    
    let args: Vec<String> = env::args().collect();
    match args[1].to_lowercase().as_str() {
        "bot" => tokio::runtime::Runtime::new().unwrap().block_on(bot::bot_main()),
        "web" => web::main(),
        "worker" => tokio::runtime::Runtime::new().unwrap().block_on(worker::main()),
        "fix_transactions" => fix_transactions::fix_transactions(),
        _ => {
            eprintln!("usage: plutocradroid <cmd>\n");
            eprintln!("available commands: bot, web, worker, fix_transactions");
            std::process::exit(100);
        }
    }
}