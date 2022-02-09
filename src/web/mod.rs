mod auctions;
mod auth;
mod bank;
mod common_context;
mod csrf;
mod deets;
mod give;
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

pub fn main() {
    rocket::ignite()
        .manage(rocket_diesel::init_pool())
        .attach(rocket_oauth2::OAuth2::<auth::DiscordOauth>::fairing("discord"))
        .attach(secure_headers::SecureHeaders)
        .mount("/", statics::statics_routes())
        .mount("/",routes![
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
        ])
        .launch();
}