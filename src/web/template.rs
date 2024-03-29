use rocket::http;
use rocket::response::{self, Responder};
use rocket::request::Request;
use diesel::prelude::*;

use super::statics::static_path;
use super::prelude::*;

#[derive(Debug,Clone)]
pub enum ErrorResponse {
    /// Normally a 400 Error, indicating an error state that shouldn't be possible to reach from the HTML provided to the client and thus doesn't need a friendly message
    HardError{status: http::Status},
    /// Always a 200 OK status, with a more friendly display and error message describing what went wrong
    SoftError{message: String},
    FourOhFour,
}

#[derive(Debug,Clone)]
pub struct OkResponse(pub maud::Markup);

pub type PlutoResponse = Result<OkResponse, ErrorResponse>;

pub fn hard_err<R>(status: http::Status) -> Result<R, ErrorResponse> {
    Err(ErrorResponse::HardError{status})
}

pub fn soft_err<R>(message: impl AsRef<str>) -> Result<R, ErrorResponse> {
    Err(ErrorResponse::SoftError{message: message.as_ref().to_string()})
}

pub fn not_found<R>() -> Result<R, ErrorResponse> {
    Err(ErrorResponse::FourOhFour)
}

pub fn bare_page(
    title: impl AsRef<str>,
    head_content: impl maud::Render,
    body_content: impl maud::Render,
) -> maud::Markup {
    maud::html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="application-name" content="CONsortium M.A.S.";
                meta name="theme-color" content="#f0f0ef";
                meta name="color-scheme" content="light dark";
                meta name="supported-color-schemes" content="light dark";
                meta name="format-detection" content="telephone=no";

                title { (title.as_ref()) }
                link rel="stylesheet" href={(static_path!(main.css))};
                link rel="icon" type="image/png" href={(static_path!(favicon.png))};

                (head_content)
            }
            body {
                img #mt-header-dark.mt-header  src=(super::statics::static_path!(mt_darkmode.png)) alt=r#"CONsortium logo header; A cube in a letter C next to the text "CONSORTIUM""#;
                img #mt-header-light.mt-header src=(super::statics::static_path!(mt_lightmode.png)) alt=r#"CONsortium logo header; A cube in a letter C next to the text "CONSORTIUM""#;
                div.container {
                    (body_content)
                    footer.build-info {
                        "Plutocradroid "
                        (env!("VERGEN_BUILD_SEMVER"))
                        " commit "
                        (env!("VERGEN_GIT_SHA"))
                        " built for "
                        (env!("VERGEN_CARGO_TARGET_TRIPLE"))
                        " at "
                        (env!("VERGEN_BUILD_TIMESTAMP"))
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PageTitle<T: AsRef<str>>(pub T);

#[derive(Debug, Clone)]
pub struct CanonicalUrl(pub Option<String>);

impl<'a> From<rocket::http::uri::Absolute<'a>> for CanonicalUrl {
    fn from(thing: rocket::http::uri::Absolute<'a>) -> Self {
        Self(Some(format!("{thing}")))
    }
}

/// Intended to be used with rocket's uri! macro to make a full URL
pub fn full_url(
    origin: rocket::http::uri::Origin<'_>
) -> rocket::http::uri::Absolute<'static> {
    use rocket::http::ext::IntoOwned;
    let url_string = format!(
        "{}{}",
        crate::SITE_URL,
        origin,
    );
    rocket::http::uri::Absolute::parse(&url_string).unwrap().into_owned()
}

pub fn page<E, T: AsRef<str>>(
    ctx: &mut super::common_context::CommonContext,
    title: PageTitle<T>,
    canonical_url: CanonicalUrl,
    head_content: impl maud::Render,
    body_content: impl maud::Render,
) -> Result<OkResponse, E> {
    use crate::schema::item_types::dsl as itdsl;
    use crate::view_schema::balance_history::dsl as bhdsl;
    let top_content = if let Some(deets) = ctx.deets.as_ref() {
        let item_types:Vec<String> = itdsl::item_types
            .select(itdsl::name)
            .order(itdsl::position)
            .get_results(&**ctx)
            .unwrap();
        let id:i64 = deets.discord_user.id();
        let balances = item_types.iter().map(|name| {
            (name,bhdsl::balance_history
                .select(bhdsl::balance)
                .filter(bhdsl::user.eq(id))
                .filter(bhdsl::ty.eq(name))
                .order(bhdsl::happened_at.desc())
                .limit(1)
                .get_result(&**ctx)
                .optional()
                .unwrap() //unwrap Result (query might fail)
                .unwrap_or(0) //unwrap Option (row might not exist)
            )
        });
        maud::html!{
            div #logged-in-header {
                "Welcome, " (deets.discord_user.username) "#" (deets.discord_user.discriminator)
            }
            form action="/logout" method="post" {
                input type="hidden" name="csrf" value=(ctx.csrf_token);
                input type="submit" name="submit" value="Logout";
            }
            details.balances {
                summary { "Tap to show balances:" }
                ul {
                    @for (name, amount) in balances {
                        li { (amount) (name) }
                    }
                }
            }
        }
    } else {
        maud::html!{
            form action="/login/discord" method="post" {
                input type="hidden" name="csrf" value=(ctx.csrf_token);
                p { 
                    "I don't know who you are. You should "
                    input type="submit" name="submit" value="Login";
                }
            }
        }
    };
    let page_content = bare_page(title.0, maud::html!{
        @if let Some(canon_url) = canonical_url.0 {
            meta name="canonical" value=(canon_url);
        }
        (head_content)
    }, maud::html!{
        header {
            (top_content)
            nav {
                a href=(uri!(super::motions::motion_index: _)) { "Motions" }
                span role="separator" aria-orientation="vertical" {
                    " | "
                }
                a href=(uri!(super::auctions::auction_index)) { "Auctions" }
                @if ctx.deets.is_some() {
                    span role="separator" aria-orientation="vertical" {
                        " | "
                    }
                    a href=(uri!(super::bank::my_transactions: fun_ty = _, before_ms = _)) { "My Transactions" }
                    span role="separator" aria-orientation="vertical" {
                        " | "
                    }
                    a href=(uri!(super::bank::give_form)) { "Transfer" }
                }    
            }
            hr;
        }
        (body_content)
    });

    Ok(OkResponse(page_content))
}

pub fn ts_plain(
    ts: DateTime<Utc>
) -> String {
    ts.with_timezone(&chrono_tz::America::Los_Angeles).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn show_ts(
    ts: DateTime<Utc>,
) -> maud::Markup {
    maud::html!{
        time datetime=(ts.to_rfc3339()) {
            (ts_plain(ts))
        }
    }
}

pub fn embed_head_html(
    meta_title: String,
    meta_description: String,
    self_uri: &rocket::http::uri::Absolute<'_>
) -> maud::Markup {
    maud::html!{
        meta property="og:title" content=(meta_title);
        meta property="og:description" content=(meta_description);
        meta property="og:type" content="website";
        meta property="og:image" content={ (crate::SITE_URL) (super::statics::static_path!(icon_twitter.png)) };
        meta property="og:image:alt" content="The CONsortium logo: a cube inside a letter C";
        meta property="og:url" content=(self_uri);
        meta property="og:site_name" content="CONsortium MAS";

        meta name="twitter:card" content="summary";
        meta name="twitter:image" content={ (crate::SITE_URL) (super::statics::static_path!(icon_twitter.png)) };
        meta name="twitter:image:alt" content="The CONsortium logo: a cube inside a letter C";
    }
}

impl<'r> Responder<'r> for ErrorResponse {
    fn respond_to(self, request: &Request<'_>) -> response::Result<'r> {
        match self {
            ErrorResponse::HardError{status} => status.respond_to(request),
            ErrorResponse::SoftError{message} =>
                bare_page(&message, maud::html!{}, &message).respond_to(request),
            ErrorResponse::FourOhFour => rocket::http::Status::NotFound.respond_to(request),
        }
    }
}

impl<'r> Responder<'r> for OkResponse {
    fn respond_to(self, request: &Request<'_>) -> response::Result<'r> {
        self.0.respond_to(request)
    }
}
