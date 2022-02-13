mod auctions;
mod auth;
mod bank;
mod common_context;
mod csrf;
mod deets;
mod misc_error;
mod motions;
mod prelude;
mod referer;
mod rocket_diesel;
mod secure_headers;
mod shortlink;
mod static_responders;
mod statics;
mod template;

#[cfg(feature = "debug")]
mod debug_utils;

use prelude::*;

#[get("/")]
fn index(
    mut ctx: CommonContext
) -> Result<template::OkResponse, !> {
    page(
        &mut ctx,
        PageTitle("CONsortium MAS"),
        full_url(uri!(index)).into(),
        html!{},
        html!{
            "Welcome to CONsortium MAS."
        },
    )
}

pub fn main() {
    let r = rocket::ignite()
        .manage(rocket_diesel::init_pool())
        .attach(rocket_oauth2::OAuth2::<auth::DiscordOauth>::fairing("discord"))
        .attach(secure_headers::SecureHeaders)
        .mount("/", statics::statics_routes())
        .mount("/", routes![
            index,
            auth::oauth_finish,
            auth::login,
            auth::get_deets,
            auth::logout,
            motions::motion_index,
            motions::motion_view,
            motions::motion_vote,
            bank::my_transactions,
            bank::give_form,
            bank::give_perform,
            auctions::auction_index,
            auctions::auction_bid,
            auctions::auction_view,
            shortlink::shortlink,
        ]);
    #[cfg(feature = "debug")]
    let r = r.mount("/", routes![
        debug_utils::debug_util_forms,
        debug_utils::impersonate,
        debug_utils::fabricate,
    ]);
    r.launch();
}