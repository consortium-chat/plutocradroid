use std::fmt::Display;
use std::fmt;
use std::collections::HashMap;
use std::borrow::Cow;
use rocket_oauth2::{OAuth2, TokenResponse};
use rocket::http::{Cookies, Cookie, SameSite};
use rocket::response::{Responder, Redirect};
use rocket::request::{FromRequest,Request,Outcome,FromQuery,Query};
use rocket::request::LenientForm;
use rocket::response::content::Content;
use rocket::response::Response;
use rocket::http::{ContentType, Status};
use rocket::fairing;
use maud::{html, Markup};
use diesel::prelude::*;
use chrono::{DateTime, Utc, SecondsFormat, TimeZone};
use serenity::model::prelude::UserId;

use crate::{schema, view_schema, rocket_diesel};
use crate::models::{Motion, MotionVote, MotionWithCount, AuctionWinner, TransferType, Transfer, TransferExtra};
use crate::bot::name_of;

trait IntoOption {
    fn into_option_lazy<T>(self, f: impl FnOnce() -> T) -> Option<T>;
    fn into_option<T>(self, v: T) -> Option<T>;
}

impl IntoOption for bool {
    fn into_option_lazy<T>(self, f: impl FnOnce() -> T) -> Option<T> {
        if self {
            Some(f())
        } else { None }
    }
    fn into_option<T>(self, v: T) -> Option<T> {
        if self {
            Some(v)
        } else { None }
    }
}

const CSRF_COOKIE_NAME:&str = "csrf_protection_token_v2";

fn generate_state<A: rand::RngCore + rand::CryptoRng>(rng: &mut A) -> Result<String, &'static str> {
    let mut buf = [0; 16]; // 128 bits
    rng.try_fill_bytes(&mut buf).map_err(|_| "Failed to generate random data")?;
    Ok(base64::encode_config(&buf, base64::URL_SAFE_NO_PAD))
}

#[derive(Debug,Copy,Clone,PartialEq,Eq)]
struct MotionFilter {
    pub pending: bool,
    pub passed: bool,
    pub failed: bool,
}

impl Default for MotionFilter {
    fn default() -> Self {
        Self{
            pending: true,
            passed: true,
            failed: true,
        }
    }
}

impl<'q> FromQuery<'q> for MotionFilter {
    type Error = !;
    fn from_query(q: Query<'q>) -> Result<Self, Self::Error> {
        let mut has_filter = false;
        let mut pending = false;
        let mut failed = false;
        let mut passed = false;

        for item in q {
            let (k,v) = item.key_value_decoded();
            if v.as_str() == "y" {
                match k.as_str() {
                    "filter" => has_filter = true,
                    "pending" => pending = true,
                    "failed" => failed = true,
                    "passed" => passed = true,
                    _ => (),
                }
            }
        }

        if has_filter {
            Ok(MotionFilter{
                pending,
                passed,
                failed,
            })
        } else {
            Ok(Default::default())
        }
    }
}

struct DiscordOauth;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CSRFToken(pub String);

#[derive(Debug, Clone, FromForm)]
struct CSRFForm {
    csrf: String
}

#[derive(Debug, Clone, FromForm)]
struct VoteForm {
    csrf: String,
    count: i64,
    direction: String,
}

#[derive(Debug, Clone, FromForm)]
struct BidForm {
    csrf: String,
    amount: u32,
}

#[derive(Debug, Clone)]
struct MiscError(String);

impl Display for MiscError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MiscError {:?}", self.0)
    }
}

impl std::error::Error for MiscError {}

impl From<String> for MiscError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for MiscError {
    fn from(s: &str) -> Self {
        Self(String::from(s))
    }
}

#[derive(Deserialize,Serialize,Debug,Clone)]
struct DiscordUser {
    pub id: String,
    pub username: String,
    pub discriminator: String,
    pub avatar: String,
}

impl DiscordUser {
    pub fn id(&self) -> i64 {
        self.id.parse().unwrap()
    }
}

#[derive(Deserialize,Serialize,Debug,Clone)]
struct Deets {
    pub discord_user: DiscordUser,
}

impl Deets {
    pub fn id(&self) -> i64 {
        self.discord_user.id()
    }
}

#[derive(Debug)]
enum DeetsFail {
    BadDeets(serde_json::error::Error),
    NoDeets
}

impl <'a, 'r> FromRequest<'a, 'r> for Deets {
    type Error = DeetsFail;

    fn from_request(request: &'a Request<'r>) -> Outcome<Self, Self::Error> {
        let mut c = request.cookies();
        //let maybe_deets = c.get("deets");
        match c.get_private("deets").map(|c| serde_json::from_str(c.value()):Result<Self,_>) {
            Some(Ok(deets)) => Outcome::Success(deets),
            Some(Err(e)) => Outcome::Failure((rocket::http::Status::BadRequest,DeetsFail::BadDeets(e))),
            None => Outcome::Failure((rocket::http::Status::Unauthorized,DeetsFail::NoDeets)),
        }
    }
}

#[derive(Debug)]
struct Referer<'a>(&'a str);

impl<'a, 'r> FromRequest<'a, 'r> for Referer<'a> {
    type Error = ();

    fn from_request(request: &'a Request<'r>) -> Outcome<Self, Self::Error> {
        match request.headers().get_one("Referer") {
            None => Outcome::Failure((rocket::http::Status::BadRequest,())),
            Some(val) => Outcome::Success(Referer(val)),
        }
    }
}

#[derive(Debug)]
enum CommonContextError {
    DeetsError(DeetsFail),
    DbConnError(()),
}

impl From<DeetsFail> for CommonContextError {
    fn from(d: DeetsFail) -> Self {
        CommonContextError::DeetsError(d)
    }
}

impl From<()> for CommonContextError {
    fn from(_: ()) -> Self {
        CommonContextError::DbConnError(())
    }
}

// If you call something the "God Object", then people get mad and say it's bad design.
// But if you call it the "context", that's fine!
struct CommonContext<'a> {
    pub csrf_token: String,
    pub cookies: Cookies<'a>,
    pub deets: Option<Deets>,
    pub conn: rocket_diesel::DbConn,
}

impl<'a> core::ops::Deref for CommonContext<'a> {
    type Target = diesel::pg::PgConnection;

    fn deref(&self) -> &Self::Target {
        self.conn.deref()
    }
}

impl <'a, 'r> FromRequest<'a, 'r> for CommonContext<'a> {
    type Error = CommonContextError;

    fn from_request(request: &'a Request<'r>) -> Outcome<Self, Self::Error> {
        let mut cookies = request.cookies();
        let csrf_token = match cookies.get(CSRF_COOKIE_NAME) {
            Some(token) => token.value().to_string(),
            None => {
                let new_token = generate_state(&mut rand::thread_rng()).unwrap();
                cookies.add(
                    Cookie::build(CSRF_COOKIE_NAME, new_token.clone())
                        .same_site(SameSite::Lax)
                        .secure(true)
                        .http_only(true)
                        .path("/")
                        .finish()
                );
                new_token
            }
        };
        let deets = match cookies.get_private("deets").map(|c| serde_json::from_str(c.value()):Result<Deets,_>) {
            Some(Ok(deets)) => Some(deets),
            Some(Err(e)) => {
                warn!("Failed to parse deets, {:?}", e);
                None
            },
            None => None,
        };

        let conn = rocket_diesel::DbConn::from_request(request).map_failure(|(a,_)| (a, CommonContextError::from(())))?;
        Outcome::Success(Self{
            csrf_token,
            cookies,
            deets,
            conn,
        })
    }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
struct SecureHeaders;

impl fairing::Fairing for SecureHeaders {
    fn info(&self) -> fairing::Info {
        fairing::Info {
            name: "Secure Headers Fairing",
            kind: fairing::Kind::Response,
        }
    }

    fn on_response(&self, _request: &Request, response: &mut Response) {
        use rocket::http::Header;
        response.adjoin_header(Header::new(
            "Content-Security-Policy",
            "default-src 'none'; frame-ancestors 'none'; img-src 'self'; script-src 'self'; style-src 'self'"
        ));
        response.adjoin_header(Header::new(
            "Referrer-Policy",
            "strict-origin-when-cross-origin"
        ));
        response.adjoin_header(Header::new(
            "X-Content-Type-Options",
            "nosniff"
        ));
        response.adjoin_header(Header::new(
            "X-Frame-Options",
            "DENY"
        ));
        // Strict-Transport-Security is purposefully omitted here; Rocket does not support SSL/TLS. The layer that is adding SSL/TLS (most likely nginx or apache) should add an appropriate STS header.
    }
}

fn show_ts(
    ts: DateTime<Utc>,
) -> Markup {
    html!{
        time datetime=(ts.to_rfc3339()) {
            (ts.with_timezone(&chrono_tz::America::Los_Angeles).to_rfc3339_opts(SecondsFormat::Secs, true))
        }
    }
}

#[allow(clippy::branches_sharing_code)]
fn motion_snippet(
    motion: &MotionWithCount
) -> Markup {
    html!{
        div.motion-titlebar {
            a href=(format!("/motions/{}", motion.damm_id())) {
                h3.motion-title { "Motion #" (motion.damm_id())}
            }
            span.motion-time {
                @if motion.announcement_message_id.is_some() {
                    @if motion.is_win {
                        "PASSED"
                    } @else {
                        "FAILED"
                    }
                    " at "
                } @else {
                    " will "
                    @if motion.is_win {
                        "pass"
                    } @else {
                        "fail"
                    }
                    " at"
                    abbr title="assuming no other result changes" { "*" }
                    " "
                }
                (show_ts(motion.end_at()))
            }
        }
        p {
            @if motion.is_super {
                "Super motion "
            } @else {
                "Simple motion "
            }
            (motion.motion_text)
        }
        div {
            @if motion.is_win {
                span.winner {
                    (motion.yes_vote_count)
                    " for "
                }
                "vs"
                span.loser {
                    " against "
                    (motion.no_vote_count)
                }
            } @else {
                span.winner {
                    (motion.no_vote_count)
                    " against "
                }
                "vs"
                span.loser {
                    " for "
                    (motion.yes_vote_count)
                }
            }
        }
    }
}

fn page(ctx: &mut CommonContext, title: impl AsRef<str>, content: Markup) -> Markup {
    use schema::item_types::dsl as itdsl;
    use crate::view_schema::balance_history::dsl as bhdsl;
    bare_page(title, html!{
        header {
            @if let Some(deets) = ctx.deets.as_ref() {
                @let item_types:Vec<String> = itdsl::item_types.select(itdsl::name).order(itdsl::position).get_results(&**ctx).unwrap();
                @let id:i64 = deets.discord_user.id();
                @let balances = item_types.iter().map(|name| {
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
                div #logged-in-header { "Welcome, " (deets.discord_user.username) "#" (deets.discord_user.discriminator)}
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
            } @else {
                form action="/login/discord" method="post" {
                    input type="hidden" name="csrf" value=(ctx.csrf_token);
                    p { 
                        "I don't know who you are. You should "
                        input type="submit" name="submit" value="Login";
                    }
                }
            }
            nav {
                a href="/" { "Motions" }
                span role="separator" aria-orientation="vertical" {
                    " | "
                }
                a href="/auctions" { "Auctions" }
                @if ctx.deets.is_some() {
                    span role="separator" aria-orientation="vertical" {
                        " | "
                    }
                    a href="/my-transactions" { "My Transactions" }
                }    
            }
            hr;
        }
        (content)
    })
}

fn bare_page(title: impl AsRef<str>, content: Markup) -> Markup {
    html! {
        (maud::DOCTYPE)
        html {
            head {
                title { (title.as_ref()) }
                link rel="stylesheet" href={"/" (static_path!(main.css))};
                link rel="icon" type="image/png" href={"/" (static_path!(favicon.png))};
            }
            body {
                div.container {
                    (content)
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


fn display_auction(auction:&AuctionWinner) -> Markup {
    html!{
        div class=(if auction.finished { "auction auction-finished" } else { "auction auction-pending" }) {
            div style="font-weight: bold" {
                a href=(format!("/auctions/{}", auction.damm())) {
                    "Auction#"
                    (auction.damm())
                }
            }
            div {
                (auction.auctioneer_name())
                @if auction.finished {
                    " offered "
                } @else {
                    " offers "
                }
                (auction.offer_amt) " " (auction.offer_ty)
                " for "
                (auction.bid_ty)
                "."
                br;
                @if auction.finished {
                    @if let Some(winner_id) = auction.winner_id {
                        "Auction won by "
                        (name_of(serenity::model::id::UserId::from(winner_id as u64)))
                        " for "
                        (auction.winner_bid.unwrap()) " " (auction.bid_ty)
                        "."
                    } @else {
                        "Auction expired with no winner."
                    }
                } @else {
                    @if let Some(winner_id) = auction.winner_id {
                        "Current bid is "
                        (auction.winner_bid.unwrap()) " " (auction.bid_ty)
                        " by "
                        (name_of(serenity::model::id::UserId::from(winner_id as u64)))
                    } @else {
                        "No bids. Minimum bid is " (auction.bid_min) " " (auction.bid_ty) "."
                    }
                    br;
                    "Auction will end at "
                    (show_ts(auction.end_at()))
                    " if no further bids are placed."
                }
            }
        }
    }
}

#[derive(Debug,Responder)]
enum RocketIsDumb {
    S(rocket::http::Status),
    R(Redirect),
    M(Markup),
}

#[post("/auctions/<damm_id>/bid", data = "<data>")]
fn auction_bid(
    mut ctx: CommonContext,
    data: LenientForm<BidForm>,
    damm_id: String,
) -> RocketIsDumb {
    let now = Utc::now();
    let id:i64;
    if let Some(digits) = crate::damm::validate_ascii(damm_id.as_str()) {
        id = atoi::atoi(digits.as_slice()).unwrap();
    } else {
        info!("bad id");
        return RocketIsDumb::S(rocket::http::Status::NotFound);
    }
    if ctx.cookies.get(CSRF_COOKIE_NAME).map(|token| token.value()) != Some(data.csrf.as_str()) {
        return RocketIsDumb::S(rocket::http::Status::BadRequest);
    }

    let deets:&Deets;
    if let Some(d) = ctx.deets.as_ref() {
        deets = d;
    } else {
        info!("no deets");
        return RocketIsDumb::S(rocket::http::Status::Unauthorized);
    }

    let mut res:Option<RocketIsDumb> = None;
    let mut fail_msg:Option<&'static str> = None;

    ctx.conn.transaction::<_,diesel::result::Error,_>(|| {
        use view_schema::balance_history::dsl as bhdsl;
        use schema::transfers::dsl as tdsl;
        use schema::auctions::dsl as adsl;
        use view_schema::auction_and_winner::dsl as anw;
        
        let maybe_auction_id:Option<i64> = adsl::auctions
        .select(adsl::rowid)
        .filter(adsl::rowid.eq(id))
        .for_update()
        .get_result(&*ctx)
        .optional()
        .unwrap();

        if maybe_auction_id.is_none() {
            res = Some(RocketIsDumb::S(rocket::http::Status::NotFound));
            return Ok(());
        }

        let auction:AuctionWinner = anw::auction_and_winner
        .select(AuctionWinner::cols())
        .filter(anw::auction_id.eq(id))
        .get_result(&*ctx)
        .unwrap();

        if now > auction.end_at() {
            fail_msg = Some("Bid failed: Auction has ended");
            return Ok(());
        }

        if data.amount < (auction.current_min_bid() as u32) {
            fail_msg = Some("Bid failed: You must bid more than that.");
            return Ok(());
        }

        if Some(deets.id()) == auction.winner_id {
            fail_msg = Some("Bid failed: You cannot increase your own bid.");
            return Ok(());
        }
        let mut to_lock = vec![];
        to_lock.push(deets.id());
        if let Some(prev_bidder) = auction.winner_id {
            if prev_bidder != deets.id() {
                to_lock.push(prev_bidder);
            }
        }
        to_lock.sort();
        for id in to_lock {
            bhdsl::balance_history
            .select(bhdsl::rowid)
            .filter(bhdsl::user.eq(id))
            .filter(bhdsl::ty.eq(&auction.bid_ty))
            .for_update()
            .execute(&*ctx)?;
        }
        let get_balance = |id| bhdsl::balance_history
        .select(bhdsl::balance)
        .filter(bhdsl::user.eq(id))
        .filter(bhdsl::ty.eq(&auction.bid_ty))
        .order(bhdsl::happened_at.desc())
        .limit(1)
        .get_result(&*ctx)
        .unwrap_or(0):i64;
        let curr_user_balance = get_balance(deets.id());
        if curr_user_balance < data.amount.into():i64 {
            fail_msg = Some("Bid failed: You do not have enough fungibles.");
            return Ok(());
        }

        if let Some(prev_winner_id) = auction.winner_id {
            let prev_winner_balance = get_balance(prev_winner_id);
            //return prev_winner's fungibles
            diesel::insert_into(tdsl::transfers).values((
                tdsl::ty.eq(&auction.bid_ty),
                tdsl::quantity.eq(&auction.winner_bid.unwrap()),
                tdsl::to_user.eq(prev_winner_id),
                tdsl::to_balance.eq(prev_winner_balance + auction.winner_bid.unwrap()),
                tdsl::happened_at.eq(diesel::dsl::now),
                tdsl::transfer_ty.eq(crate::models::TransferType::AuctionRefund),
                tdsl::auction_id.eq(&auction.auction_id),
            )).execute(&*ctx).unwrap();
        }

        diesel::insert_into(tdsl::transfers).values((
            tdsl::ty.eq(&auction.bid_ty),
            tdsl::quantity.eq(data.amount as i64),
            tdsl::from_user.eq(deets.id()),
            tdsl::from_balance.eq(curr_user_balance - (data.amount as i64)),
            tdsl::happened_at.eq(diesel::dsl::now),
            tdsl::transfer_ty.eq(crate::models::TransferType::AuctionReserve),
            tdsl::auction_id.eq(auction.auction_id),
        )).execute(&*ctx).unwrap();

        res = Some(RocketIsDumb::R(Redirect::to(format!("/auctions/{}", damm_id))));
        Ok(())
    }).unwrap();

    if let Some(fail_msg) = fail_msg {
        RocketIsDumb::M(page(&mut ctx, "Auction bid failed", html!{
            main { (fail_msg) }
            br;
            a href={"/auctions/" (damm_id)} { "Return to auction" }
            br;
            a href="/" { "Return home" }
        }))
    } else {
        res.unwrap()
    }
}

#[get("/auctions/<damm_id>")]
fn auction_view(
    damm_id: String,
    mut ctx: CommonContext,
) -> impl Responder<'static> {
    let id:i64;
    if let Some(digits) = crate::damm::validate_ascii(damm_id.as_str()) {
        id = atoi::atoi(digits.as_slice()).unwrap();
    } else {
        return None;
    }
    use crate::models::AuctionWinner;
    use crate::view_schema::auction_and_winner::dsl as anw;
    use crate::schema::transfers::dsl as tdsl;

    let maybe_auction:Option<AuctionWinner> = anw::auction_and_winner
    .select(AuctionWinner::cols())
    .filter(anw::auction_id.eq(id))
    .get_result(&*ctx)
    .optional()
    .unwrap();

    let auction;
    if let Some(a) = maybe_auction {
        auction = a;
    } else {
        return None;
    }
    let transaction_history:Vec<Transfer> = tdsl::transfers
        .select(Transfer::cols())
        .filter(tdsl::auction_id.eq(auction.auction_id))
        .order((tdsl::happened_at.asc(), tdsl::rowid.asc()))
        .get_results(&*ctx)
        .unwrap();
    
    let mut auction_history:Vec<(DateTime<Utc>, String)> = vec![];
    auction_history.push((
        auction.created_at,
        format!(
            "Auction created by {}",
            auction.auctioneer_name(),
        )
    ));
    for t in &transaction_history {
        match &t.extra {
            TransferExtra::AuctionCreate{ auction_id: _, from: _} => (), // Already covered above
            TransferExtra::AuctionReserve{auction_id: _, from} => {
                auction_history.push((t.happened_at, format!(
                    "{} bids {} {}",
                    name_of(from.discord_id()),
                    t.quantity,
                    auction.bid_ty,
                )))
            },
            TransferExtra::AuctionRefund{ auction_id: _, to: _} => (), // We don't need to show anything, as this always happens at the same instant as an AuctionReserve
            TransferExtra::AuctionPayout{ auction_id: _, to} => {
                auction_history.push(
                    (t.happened_at, format!(
                        "{} wins the auction, receiving {} {}.",
                        name_of(to.discord_id()),
                        t.quantity,
                        auction.offer_ty,
                    ))
                )
            },
            _ => unreachable!(),
        }
    }

    let content = html!{
        main {
            (display_auction(&auction))
            @if !auction.finished {
                @if ctx.deets.is_some() {
                    form action={"/auctions/" (damm_id) "/bid"} method="post" {
                        input type="hidden" name="csrf" value=(ctx.csrf_token.clone());
                        "Bid "
                        input type="number" name="amount" min=(auction.current_min_bid()) value=(auction.current_min_bid());
                        (auction.bid_ty)
                        br;
                        button type="submit" { "Place bid" }
                    }
                } @else {
                    div { "Log in to bid" }
                }
            }
            h2 { "Auction history" }
            table.tabley-table {
                tr {
                    th { "At" }
                    th {}
                }
                @for (happened_at, msg) in auction_history {
                    tr {
                        td {
                            (show_ts(happened_at))
                        }
                        td{ (msg) }
                    }
                }
            }
        }
    };

    Some(page(&mut ctx, format!("Auction#{}", damm_id), content))
}

#[get("/auctions")]
fn auction_index(
    mut ctx: CommonContext,
) -> impl Responder<'static> {
    use crate::view_schema::auction_and_winner::dsl as anw;
    let pending_auctions:Vec<AuctionWinner> =
        anw::auction_and_winner
        .select(AuctionWinner::cols())
        .filter(anw::finished.eq(false))
        .order((
            anw::created_at.desc(),
        ))
        .get_results(&*ctx)
        .unwrap()
    ;
    let finished_auctions:Vec<AuctionWinner> =
        anw::auction_and_winner
        .select(AuctionWinner::cols())
        .filter(anw::finished.eq(true))
        .order((
            anw::created_at.desc(),
        ))
        .get_results(&*ctx)
        .unwrap()
    ;
    page(&mut ctx, "Auctions", html!{
        h1 { "Auctions" }
        h2 { "Pending auctions" }
        @for auction in pending_auctions {
            article { (display_auction(&auction)) }
        }

        hr;
        h2 { "Finished auctions" }
        @for auction in finished_auctions {
            article { (display_auction(&auction)) }
        }
    })
}

#[post("/motions/<damm_id>/vote", data = "<data>")]
fn motion_vote(
    mut ctx: CommonContext,
    data: LenientForm<VoteForm>,
    damm_id: String,
) -> impl Responder<'static> {
    let id:i64;
    if let Some(digits) = crate::damm::validate_ascii(damm_id.as_str()) {
        id = atoi::atoi(digits.as_slice()).unwrap();
    } else {
        info!("bad id");
        return Err(rocket::http::Status::NotFound);
    }
    if ctx.cookies.get(CSRF_COOKIE_NAME).map(|token| token.value()) != Some(data.csrf.as_str()) {
        return Err(rocket::http::Status::BadRequest);
    }
    let deets:&Deets;
    if let Some(d) = ctx.deets.as_ref() {
        deets = d;
    } else {
        info!("no deets");
        return Err(rocket::http::Status::Unauthorized);
    }
    let vote_count = data.count;
    let vote_direction:bool;
    if data.direction.as_str() == "for" {
        vote_direction = true;
    } else if data.direction.as_str() == "against" {
        vote_direction = false;
    } else {
        info!("bad vote direction {:?}", data.direction);
        return Err(rocket::http::Status::BadRequest);
    }
    let resp = crate::bot::vote_common(
        &ctx.conn, 
        Some(vote_direction),
        vote_count,
        deets.discord_user.id(),
        Some(id),
        None,
        None
    );

    Ok(page(&mut ctx, "Vote Complete", html!{
        main { (resp) }
        br;
        a href={"/motions/" (damm_id)} { "Back to Motion" }
        br;
        a href="/" { "Back Home" }
    }))
}

#[get("/motions/<damm_id>")]
fn motion_listing(mut ctx: CommonContext, damm_id: String) -> impl Responder<'static> {
    let id:i64;
    if let Some(digits) = crate::damm::validate_ascii(damm_id.as_str()) {
        id = atoi::atoi(digits.as_slice()).unwrap();
    } else {
        return None;
    }

    use schema::motions::dsl as mdsl;
    use schema::motion_votes::dsl as mvdsl;
    use schema::transfers::dsl as tdsl;
    let maybe_motion:Option<Motion> = mdsl::motions.select(Motion::cols()).filter(mdsl::rowid.eq(id)).get_result(&*ctx).optional().unwrap();
    
    let motion;
    if let Some(m) = maybe_motion {
        motion = m;
    }else{
        return None;
    }

    let votes:Vec<MotionVote> = mvdsl::motion_votes
        .select(MotionVote::cols())
        .filter(mvdsl::motion.eq(motion.rowid))
        .get_results(&*ctx)
        .unwrap();
    let (yes_vote_count, no_vote_count) = votes
        .iter()
        .map(|v| if v.direction { (v.amount, 0) } else { (0, v.amount) })
        .fold((0,0), |acc, x| (acc.0 + x.0, acc.1 + x.1));
    let vote_directions:HashMap<i64, bool> = votes
        .iter()
        .map(|v| (v.user, v.direction))
        .collect();
    let transaction_history:Vec<Transfer> = tdsl::transfers
        .select(Transfer::cols())
        .filter(tdsl::to_motion.eq(motion.rowid))
        .order(tdsl::happened_at.asc())
        .get_results(&*ctx)
        .unwrap();
    let motion = MotionWithCount::from_motion(motion, yes_vote_count as u64, no_vote_count as u64);
    let voting_html = if let Some(deets) = ctx.deets.as_ref(){
        if motion.end_at() > Utc::now() {
            let mut agents_vote:Option<MotionVote> = None;
            for vote in &votes {
                if vote.user == atoi::atoi::<i64>(deets.discord_user.id.as_bytes()).unwrap() {
                    agents_vote = Some(*vote);
                }
            }
            let avd = agents_vote.map(|v| v.direction);
            html!{
                form action={"/motions/" (damm_id) "/vote"} method="post" {
                    input type="hidden" name="csrf" value=(ctx.csrf_token);
                    "Cast "
                    input type="number" name="count" value="0";
                    " vote(s) "
                    br;
                    label {
                    input type="radio" name="direction" value="for" disabled?[avd == Some(false)] checked?[avd == Some(true)];
                    " for"
                    }
                    br;
                    label {
                        input type="radio" name="direction" value="against" disabled?[avd == Some(true)] checked?[avd == Some(false)];
                        " against"
                    }
                    br;
                    input type="submit" name="submit" value="Go";
                }
            }
        } else {
            html!{ "This motion has expired." }
        }
    } else {
        html!{ "You must be logged in to vote." }
    };

    let mut motion_history:Vec<(DateTime<Utc>,Cow<'static, str>,String)> = vec![];

    for t in transaction_history {
        match t.extra {
            TransferExtra::Motion{from, motion_id: _, votes, created} => {
                motion_history.push((
                    t.happened_at,
                    name_of(from.discord_id()),
                    if created {
                        format!("Created this motion with {} vote(s).", votes)
                    } else {
                        format!(
                            "Voted {} this motion {} time(s).",
                            if vote_directions[&from.user] {
                                "in favor"
                            } else {
                                "against"
                            },
                            (format!("{}",votes))
                        )
                    }
                ))
            },
            _ => unreachable!(),
        }
    }

    if motion.end_at() < Utc::now() {
        motion_history.push((
            motion.end_at(),
            "".into(),
            format!(
                "Motion {}.",
                if motion.is_win {
                    "passed"
                } else {
                    "failed"
                }
            ),
        ))
    }

    #[allow(unreachable_code)]
    let markup:Markup = html!{
        main {
            h1 { (format!(
                "Motion #{}",
                motion.damm_id(),
            )) }
            div.motion {
                (motion_snippet(&motion))
                hr;
                (voting_html)
                hr;
                dl.motion-votes {
                    @for vote in &votes {
                        dt { (name_of(UserId::from(vote.user as u64))) }
                        dd {
                            (vote.amount)
                            @if vote.direction {
                                " for"
                            } @else {
                                " against"
                            }
                        }
                    }
                }
            }
            h2 { "Motion History" }
            table.motion-history.tabley-table {
                thead {
                    tr {
                        th { "Timestamp" }
                        th { "User" }
                        th {}
                    }
                }
                tbody {
                    @for (date, user, msg) in motion_history {
                        tr {
                            td { (show_ts(date)) }
                            td { (user) }
                            td { (msg) }
                        }
                    }
                }
            }
        }
    };

    Some(page(&mut ctx, format!("Motion #{}", motion.damm_id()), markup))
}

#[get("/?<filters..>")]
fn index(
    mut ctx: CommonContext,
    filters: MotionFilter,
) -> impl Responder<'static> {

    use schema::motions::dsl as mdsl;
    use schema::motion_votes::dsl as mvdsl;
    let bare_motions:Vec<Motion> = mdsl::motions
        .select(Motion::cols())
        .order((mdsl::announcement_message_id.is_null().desc(), mdsl::rowid.desc()))
        .get_results(&*ctx)
        .unwrap();

    let get_vote_count = |motion_id:i64, dir:bool| -> Result<i64, diesel::result::Error> {
        use bigdecimal::{BigDecimal,ToPrimitive};
        let votes:Option<BigDecimal> = mvdsl::motion_votes
        .select(diesel::dsl::sum(mvdsl::amount))
        .filter(mvdsl::motion.eq(motion_id))
        .filter(mvdsl::direction.eq(dir))
        .get_result(&*ctx)?;
        Ok(votes.map(|bd| bd.to_i64().unwrap()).unwrap_or(0))
    };

    let motions = (bare_motions.into_iter().map(|m| {
        let yes_votes = get_vote_count(m.rowid, true)?;
        let no_votes = get_vote_count(m.rowid, false)?;
        Ok(MotionWithCount::from_motion(m, yes_votes as u64, no_votes as u64))
    }).collect():Result<Vec<_>,diesel::result::Error>)
        .unwrap()
        .into_iter()
        .filter(|m| {
            (filters.pending && m.announcement_message_id.is_none()) ||
            (filters.passed && m.announcement_message_id.is_some() && m.is_win) ||
            (filters.failed && m.announcement_message_id.is_some() && !m.is_win)
        })
        .collect():Vec<_>;

    page(&mut ctx, "All Motions", html!{
        h1 { "All Motions" }
        "Filters:"

        form.tall-form {
            input type="hidden" name="filter" value="y";
            label {
                input type="checkbox" name="pending" value="y" checked[filters.pending];
                "Pending"
            }
            label {
                input type="checkbox" name="passed" value="y" checked[filters.passed];
                "Passed"
            }
            label {
                input type="checkbox" name="failed" value="y" checked[filters.failed];
                "Failed"
            }
            button."mt-1" type="submit" { "Go" }
        }
        main {
            @for motion in &motions {
                article.motion {
                    (motion_snippet(motion))
                }
            }
            @if motions.is_empty() {
                p.no-motions { "Nobody here but us chickens!" }
            }
        }
    })
}

sql_function!{
    #[sql_name = "coalesce"]
    fn coalesce_2<T: diesel::sql_types::NotNull>(a: diesel::sql_types::Nullable<T>, b: T) -> T;
}
// use diesel::sql_types::Bool;
// sql_function!{
//     #[sql_name = "coalesce"]
//     fn coalesce_2_bool(a: diesel::sql_types::Nullable<Bool>, b: Bool) -> Bool;
// }

#[get("/my-transactions?<before_ms>&<fun_ty>")]
fn my_transactions(
    mut ctx: CommonContext,
    fun_ty: Option<String>,
    before_ms: Option<i64>,
) -> Result<Markup, Status> {
    use crate::view_schema::balance_history::dsl as bh;
    use crate::schema::item_types::dsl as it;
    let before_ms = before_ms.unwrap_or(i64::MAX);
    #[cfg(feature = "debug")]
    let limit = 10;
    #[cfg(not(feature = "debug"))]
    let limit = 1000;
    let fun_ty_string = fun_ty.unwrap_or_else(|| String::from("all"));
    #[derive(Debug,Clone,PartialEq,Eq)]
    enum FungibleSelection {
        All,
        Specific(String),
    }
    
    impl FungibleSelection {
        pub fn as_str(&self) -> &str {
            match self {
                FungibleSelection::All => "all",
                FungibleSelection::Specific(s) => s,
            }
        }

        pub fn as_option(&self) -> Option<&str> {
            match self {
                FungibleSelection::All => None,
                FungibleSelection::Specific(s) => Some(s.as_str()),
            }
        }
    }
    #[derive(Debug,Clone,Queryable)]
    struct Transaction {
        //pub rowid:i64,
        pub balance:i64,
        pub quantity:i64,
        pub sign:i32,
        pub happened_at:DateTime<Utc>,
        pub ty:String,
        pub comment:Option<String>,
        pub other_party:Option<i64>,
        pub to_motion:Option<i64>,
        pub to_votes:Option<i64>,
        //pub message_id:Option<i64>,
        pub transfer_ty:TransferType,
        pub auction_id:Option<i64>,
    }
    let transaction_cols = (
        //bh::rowid,
        bh::balance,
        bh::quantity,
        bh::sign,
        bh::happened_at,
        bh::ty,
        bh::comment,
        bh::other_party,
        bh::to_motion,
        bh::to_votes,
        //bh::message_id,
        bh::transfer_ty,
        bh::auction_id,
    );
    #[derive(Debug,Clone)]
    enum TransactionView {
        Generated{amt: i64, bal: i64},
        Trans(Transaction),
    }
    let fun_tys:Vec<String> = it::item_types.select(it::name).order(it::position).get_results(&*ctx).unwrap();
    let fun_ty = if fun_ty_string == "all" {
        FungibleSelection::All
    } else if fun_tys.iter().any(|ft| ft.as_str() == fun_ty_string) {
        FungibleSelection::Specific(fun_ty_string)
    } else {
        return Err(Status::BadRequest)
    };
    let txns:Option<(Vec<_>,bool)> = ctx.deets.as_ref().map(|deets| {
        let q = bh::balance_history
            .select(transaction_cols)
            .filter(bh::user.eq(deets.id()))
            .filter(coalesce_2(bh::ty.nullable().eq(fun_ty.as_option()).nullable(), true))
            .filter(coalesce_2(bh::happened_at.nullable().lt(Utc.timestamp_millis_opt(before_ms).single()).nullable(),true))
            .filter(bh::transfer_ty.ne(TransferType::Generated))
            .order(bh::happened_at.desc())
            .limit(limit+1);
        info!("{}", diesel::debug_query(&q));
        let txns:Vec<Transaction> = q.get_results(&*ctx)
            .unwrap();
        info!("{} txns results", txns.len());
        let mut gen_txns:Vec<Transaction> = if let [.., last] = txns.as_slice() {
            bh::balance_history
                .select(transaction_cols)
                .filter(bh::user.eq(deets.id()))
                .filter(coalesce_2(bh::ty.nullable().eq(fun_ty.as_option()).nullable(), true))
                .filter(coalesce_2(bh::happened_at.nullable().lt(Utc.timestamp_millis_opt(before_ms).single()).nullable(),true))
                .filter(bh::happened_at.gt(last.happened_at))
                .filter(bh::transfer_ty.eq(TransferType::Generated))
                .order(bh::happened_at.desc())
                .get_results(&*ctx)
                .unwrap()
        } else { Vec::new() };
        let mut txn_views = Vec::new();
        let (hit_limit,iter) = if txns.len() == ((limit+1) as usize) {
            (true, txns[..txns.len()-1].iter())
        } else { (false, txns.iter()) };
        for txn in iter.rev() {
            let mut amt = 0;
            let mut bal = 0;
            while gen_txns.last().map(|t| t.happened_at < txn.happened_at).unwrap_or(false) {
                let gen_txn = gen_txns.pop().unwrap();
                amt += gen_txn.quantity;
                bal = gen_txn.balance;
            }
            if amt > 0 {
                txn_views.push(TransactionView::Generated{amt, bal});
            }
            txn_views.push(TransactionView::Trans(txn.clone()));
        }
        let mut amt = 0;
        let mut bal = 0;
        while let Some(gt) = gen_txns.pop() {
            amt += gt.quantity;
            bal = gt.balance;
        }
        if amt > 0 {
            txn_views.push(TransactionView::Generated{amt,bal});
        }
        txn_views.reverse();
        (txn_views, hit_limit)
    });
    Ok(page(&mut ctx, "My Transactions", html!{
        main {
            h1 { "My Transactions" }
            @if let Some((txns, hit_limit)) = txns {
                form.tall-form {
                    div { "Show transactions in:" }
                    @for ft in &fun_tys {
                        label {
                            input type="radio" name="fun_ty" value=(ft) checked?[fun_ty == FungibleSelection::Specific(ft.clone())];
                            (ft)
                        }
                    }
                    label {
                        input type="radio" name="fun_ty" value="all" checked?[fun_ty == FungibleSelection::All];
                        "All currencies"
                    }
                    .spacer-tall {}
                    button type="submit" { "Go" }
                    .spacer-tall {}
                }
                table.tabley-table {
                    thead {
                        tr {
                            th { "Timestamp" }
                            th { "Description" }
                            th { "Amount" }
                            th { "Running Total" }
                        }
                    }
                    tbody {
                        @for txn_view in &txns {
                            @if let TransactionView::Trans(txn) = txn_view {
                                tr.transaction {
                                    td {
                                        (show_ts(txn.happened_at))
                                    }
                                    td {
                                        @match txn.transfer_ty {
                                            TransferType::Give | TransferType::AdminGive => {
                                                @if txn.transfer_ty == TransferType::AdminGive {
                                                    "admin "
                                                }
                                                @if txn.sign < 0 {
                                                    "transfer to "
                                                } @else {
                                                    "transfer from "
                                                }
                                                "user#\u{200B}"
                                                (txn.other_party.unwrap())
                                            },
                                            TransferType::MotionCreate => {
                                                @let damm_id = crate::damm::add_to_str(txn.to_motion.unwrap().to_string());
                                                "1 vote, created "
                                                a href=(uri!(motion_listing:damm_id = &damm_id)) {
                                                    "motion #"
                                                    (&damm_id)
                                                }
                                            },
                                            TransferType::MotionVote => {
                                                @let motion_id = &txn.to_motion.unwrap();
                                                @let votes = &txn.to_votes.unwrap();
                                                @let damm_id = crate::damm::add_to_str(motion_id.to_string());
                                                (votes)
                                                " vote(s) on "
                                                a href=(uri!(motion_listing:damm_id = &damm_id)) {
                                                    "motion #"
                                                    (&damm_id)
                                                }
                                            },
                                            TransferType::AdminFabricate | TransferType::CommandFabricate => {
                                                "fabrication"
                                            },
                                            TransferType::AuctionCreate => {
                                                @let damm_id = crate::damm::add_to_str(txn.auction_id.unwrap().to_string());
                                                "Created "
                                                a href=(uri!(auction_view:damm_id = &damm_id)) {
                                                    "auction #"
                                                    (&damm_id)
                                                }
                                            },
                                            TransferType::AuctionReserve => {
                                                @let damm_id = crate::damm::add_to_str(txn.auction_id.unwrap().to_string());
                                                "Bid on "
                                                a href=(uri!(auction_view:damm_id = &damm_id)) {
                                                    "auction #"
                                                    (&damm_id)
                                                }
                                            },
                                            TransferType::AuctionRefund => {
                                                @let damm_id = crate::damm::add_to_str(txn.auction_id.unwrap().to_string());
                                                "Outbid on "
                                                a href=(uri!(auction_view:damm_id = &damm_id)) {
                                                    "auction #"
                                                    (&damm_id)
                                                }
                                            },
                                            TransferType::AuctionPayout => {
                                                @let damm_id = crate::damm::add_to_str(txn.auction_id.unwrap().to_string());
                                                "Won the bid, payout for "
                                                a href=(uri!(auction_view:damm_id = &damm_id)) {
                                                    "auction #"
                                                    (&damm_id)
                                                }
                                            },
                                            TransferType::Generated => "unreachable",
                                        }
                                        // @if txn.transfer_ty == TransferType::Give || txn.transfer_ty == TransferType::AdminGive {
                                        // } @else if txn.transfer_ty == TransferType::MotionCreate {
                                        // } @else if let (Some(motion_id), Some(votes)) = (&txn.to_motion, &txn.to_votes) {
                                        //     assert_eq!(txn.transfer_ty, MotionVote);
                                        // } @else if txn.transfer_ty == TransferType::AdminFabricate || txn.transfer_ty == TransferType::CommandFabricate {
                                        //     "fabrication"
                                        // }
                                        // " "
                                        @if let Some(comment) = &txn.comment {
                                            " “" (comment) "”"
                                        }
                                    }
                                    td.amount.negative[txn.sign < 0] {
                                        span.paren { "(" }
                                        span.amount-inner { (txn.quantity) }
                                        span.ty { (txn.ty) }
                                        span.paren { ")" }
                                    }
                                    td.running-total {
                                        span.amount-inner { (txn.balance) }
                                        span.ty { (txn.ty) }
                                    }
                                }
                            } @else {
                                @let (amt, bal) = match txn_view { TransactionView::Generated{amt, bal} => (amt, bal), _ => unreachable!() };
                                tr.transaction.generated {
                                    td {}
                                    td { "generator outputs" }
                                    td.amount {
                                        span.paren { "(" }
                                        span.amount-inner { (amt) }
                                        span.ty { "pc" }
                                        span.paren { ")" }
                                    }
                                    td.running-total {
                                        span.amount-inner { (bal) }
                                        span.ty { "pc" }
                                    }
                                }
                            }
                        }
                        @if txns.is_empty() {
                            tr {
                                td colspan="4" {
                                    "Nothing to show."
                                }
                            }
                        }
                    }
                }
                @if hit_limit {
                    @let txn = match txns.iter().rev().find(|t| matches!(t, TransactionView::Trans(_))) { Some(TransactionView::Trans(t)) => t, d => {dbg!(d);unreachable!()} };
                    a href=(uri!(my_transactions: before_ms = txn.happened_at.timestamp_millis(), fun_ty = fun_ty.as_str())) { "Next" }
                }
            } @else {
                p { "You must be logged in to view your transactions." }
            }
        }
    }))
}

/// This is the 1st step in a 3-step process to a discord OAUTH login.
/// It stores the URL to eventually redirect back to at the end in a cookie, then redirects to discord.
/// From there, the agent logs into discord and authorizes the app. Discord then redirects to /oauth-finish
#[post("/login/discord", data = "<data>")]
fn login(
    oauth2: OAuth2<DiscordOauth>,
    mut cookies: Cookies<'_>,
    maybe_referer: Option<Referer>,
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
fn oauth_finish(token: TokenResponse<DiscordOauth>, mut cookies: Cookies<'_>) -> Redirect {
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
fn get_deets(
    mut cookies: Cookies<'_>
) -> Result<Redirect, Box<dyn std::error::Error>> {
    let token;
    if let Some(val) = cookies.get_private("token") {
        token = val.value().to_string()
    } else {
        return Ok(Redirect::to("/"));
    }
    let client = reqwest::blocking::Client::new();
    let res = client.get("https://discord.com/api/v8/users/@me")
        .bearer_auth(token)
        .send()?;
    if res.status() != 200 {
        return Err(Box::new(MiscError::from("Bad status")));
    }
    let user:DiscordUser = res.json()?;
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
fn logout(
    mut ctx: CommonContext,
    data: LenientForm<CSRFForm>,
) -> Result<Markup, rocket::http::Status> {
    if ctx.cookies.get(CSRF_COOKIE_NAME).map(|token| token.value()) != Some(data.csrf.as_str()) {
        return Err(rocket::http::Status::BadRequest);
    }
    let cookies_clone = ctx.cookies.iter().map(Clone::clone).collect():Vec<_>;
    for cookie in cookies_clone {
        ctx.cookies.remove(cookie);
    }
    Ok(bare_page("Logged out.", html!{
        p { "You have been logged out." }
        a href="/" { "Home." }
    }))
}

#[get("/motions")]
fn motions_api_compat(
    ctx: CommonContext
) -> impl Responder {
    use schema::motions::dsl as mdsl;
    use schema::motion_votes::dsl as mvdsl;
    let bare_motions:Vec<Motion> = mdsl::motions.select(Motion::cols()).get_results(&*ctx).unwrap();

    let get_vote_count = |motion_id:i64, dir:bool| -> Result<i64, diesel::result::Error> {
        use bigdecimal::{BigDecimal,ToPrimitive};
        let votes:Option<BigDecimal> = mvdsl::motion_votes
        .select(diesel::dsl::sum(mvdsl::amount))
        .filter(mvdsl::motion.eq(motion_id))
        .filter(mvdsl::direction.eq(dir))
        .get_result(&*ctx)?;
        Ok(votes.map(|bd| bd.to_i64().unwrap()).unwrap_or(0))
    };

    let res = (bare_motions.into_iter().map(|m| {
        let yes_votes = get_vote_count(m.rowid, true)?;
        let no_votes = get_vote_count(m.rowid, false)?;
        Ok(MotionWithCount::from_motion(m, yes_votes as u64, no_votes as u64))
    }).collect():Result<Vec<_>,diesel::result::Error>).unwrap();
    
    Content(ContentType::JSON, serde_json::to_string(&res).unwrap())
}

fn full_url(
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

// Intended for nginx to internally redirect from shortlink domains, but if someone goes here directly that's fine
#[get("/shortlink/<damm_id>")]
fn shortlink(
    damm_id: String,
    ctx: CommonContext,
) -> Option<Redirect> {
    let id:i64;
    if let Some(digits) = crate::damm::validate_ascii(damm_id.as_str()) {
        id = atoi::atoi(digits.as_slice()).unwrap();
    } else {
        return None;
    }

    use schema::motions::dsl as mdsl;
    use schema::auctions::dsl as adsl;
    use diesel::dsl::{select, exists};


    let is_motion = select(exists(mdsl::motions.filter(mdsl::rowid.eq(id)))).get_result(&*ctx).unwrap();
    if is_motion {
        return Some(Redirect::permanent(full_url(uri!(motion_listing: damm_id = damm_id))));
    }

    let is_auction = select(exists(adsl::auctions.filter(adsl::rowid.eq(id)))).get_result(&*ctx).unwrap();
    if is_auction {
        return Some(Redirect::permanent(full_url(uri!(auction_view:   damm_id = damm_id))));
    }

    None
}

pub fn main() {
    rocket::ignite()
        .manage(rocket_diesel::init_pool())
        .attach(OAuth2::<DiscordOauth>::fairing("discord"))
        .attach(SecureHeaders)
        .mount("/", super::statics::statics_routes())
        .mount("/",routes![
            index,
            oauth_finish,
            login,
            get_deets,
            motion_listing,
            motion_vote,
            motions_api_compat,
            logout,
            my_transactions,
            auction_index,
            auction_bid,
            auction_view,
            shortlink,
        ])
        .launch();
}