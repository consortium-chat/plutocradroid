use rocket::request::LenientForm;
use rocket::http::{Cookie,Cookies,SameSite};
use rocket::http::Status;
use rocket::response::Redirect;
use rocket_oauth2::{OAuth2,TokenResponse};

use super::csrf::*;
use super::template;
use super::deets::{Deets, DiscordUser};
use super::common_context::CommonContext;

pub struct DiscordOauth;

/// This is the 1st step in a 3-step process to a discord OAUTH login.
/// It stores the URL to eventually redirect back to at the end in a cookie, then redirects to discord.
/// From there, the agent logs into discord and authorizes the app. Discord then redirects to /oauth-finish
#[post("/login/discord", data = "<data>")]
pub fn login(
    oauth2: OAuth2<DiscordOauth>,
    mut cookies: rocket::http::Cookies<'_>,
    maybe_referer: Option<super::referer::Referer>,
    data: LenientForm<CSRFForm>,
) -> Result<Redirect, rocket::http::Status> {
    if cookies.get(CSRF_COOKIE_NAME).map(|token| token.value()) != Some(data.csrf.as_str()) {
        info!("Bad csrf token");
        return Err(rocket::http::Status::BadRequest);
    }
    if let Some(referer) = maybe_referer {
        cookies.add(Cookie::build("login_redirect", referer.0.to_string()).finish());
    } else {
        cookies.remove(Cookie::named("login_redirct"));
    }
    
    Ok(oauth2.get_redirect(&mut cookies, &["identify"]).unwrap())
}

/// This is the 2nd step in a 3-step process
/// Agent has just been redirected from discord, and the url params includes a token we need to auth with discord.
/// Sets cookies and redirects to /get-deets
#[get("/oauth-finish")]
pub fn oauth_finish(token: TokenResponse<DiscordOauth>, mut cookies: Cookies<'_>) -> Redirect {
    cookies.add_private(
        Cookie::build("token", token.access_token().to_string())
            .same_site(SameSite::Lax)
            .secure(true)
            .http_only(true)
            .finish()
    );
    if let Some(refresh) = token.refresh_token().map(|s| s.to_owned()) {
        cookies.add_private(
            Cookie::build("refresh", refresh)
                .same_site(SameSite::Lax)
                .secure(true)
                .http_only(true)
                .finish()
        )
    }
    Redirect::to("/get-deets")
}

/// This is the 3rd step in a 3-step process
/// There's no reason this should exist. But for some reason it just wasn't working otherwise.
/// In theory, this should all be something I could do in /oauth-finish
/// This makes the user wait extra long for no good reason.
/// This asks discord (synchonously, mumble grumble) for users details (username, discriminator, id)
/// and stores that in the "deets" cookie. The agent is "logged in"
/// Redirects to whatevers in "login_redirect" or / if unset
#[get("/get-deets")]
pub fn get_deets(
    mut cookies: Cookies<'_>
) -> Result<Redirect, template::ErrorResponse> {
    let token = if let Some(val) = cookies.get_private("token") {
        val.value().to_string()
    } else {
        return template::hard_err(Status::BadRequest);
    };
    let client = reqwest::blocking::Client::new();
    let res = client.get("https://discord.com/api/v8/users/@me")
        .bearer_auth(token)
        .send()
        .unwrap();
    if res.status() != 200 {
        return template::hard_err(Status::InternalServerError);
    }
    let user:DiscordUser = res.json().unwrap();
    let deets = Deets{discord_user: user};
    info!("User logged in: {:?}", deets);
    cookies.add_private(
        Cookie::build("deets", serde_json::to_string(&deets).unwrap())
            .same_site(SameSite::Lax)
            .secure(true)
            .http_only(true)
            .finish()
    );
    match cookies.get("login_redirect") {
        None => Ok(Redirect::to("/")),
        Some(c) => {
            let url = c.value().to_string();
            let c = c.clone();
            cookies.remove(c);
            Ok(Redirect::to(url))
        },
    }
}
 
#[post("/logout", data = "<data>")]
pub fn logout(
    mut ctx: CommonContext,
    data: LenientForm<CSRFForm>,
) -> Result<maud::Markup, rocket::http::Status> {
    if ctx.cookies.get(CSRF_COOKIE_NAME).map(|token| token.value()) != Some(data.csrf.as_str()) {
        return Err(rocket::http::Status::BadRequest);
    }
    let cookies_clone = ctx.cookies.iter().map(Clone::clone).collect():Vec<_>;
    for cookie in cookies_clone {
        ctx.cookies.remove(cookie);
    }
    Ok(template::bare_page("Logged out.", maud::html!{}, maud::html!{
        p { "You have been logged out." }
        a href="/" { "Home." }
    }))
}