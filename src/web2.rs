use std::fmt::Display;
use std::fmt;
use rocket_oauth2::{OAuth2, TokenResponse};
use rocket::http::{Cookies, Cookie, SameSite};
use rocket::response::{Responder, Redirect};
use rocket::request::{FromRequest,Request,Outcome};
use rocket::request::LenientForm;
use rocket::response::content::Content;
use rocket::response::Response;
use rocket::http::ContentType;
use rocket::http::RawStr;
use rocket::fairing;
use rocket_contrib::serve::StaticFiles;
use maud::{html, Markup};
use diesel::prelude::*;

use crate::{schema, rocket_diesel};
use crate::models::{Motion, MotionVote, MotionWithCount};

fn generate_state<A: rand::RngCore + rand::CryptoRng>(rng: &mut A) -> Result<String, String> {
    let mut buf = [0; 16]; // 128 bits
    rng.try_fill_bytes(&mut buf).map_err(|_| {
        String::from("Failed to generate random data")
    })?;
    Ok(base64::encode_config(&buf, base64::URL_SAFE_NO_PAD))
}

#[derive(Debug,Copy,Clone,PartialEq,Eq)]
enum MotionListFilter {
    All,
    Passed,
    Failed,
    Finished,
    Pending,
    PendingPassed,
}

impl Default for MotionListFilter {
    fn default() -> Self {
        MotionListFilter::All
    }
}

impl<'v> rocket::request::FromFormValue<'v> for MotionListFilter {
    type Error = &'v RawStr;
    fn from_form_value(v: &'v RawStr) -> Result<Self, Self::Error> {
        match v.as_str() {
            "all" => Ok(Self::All),
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            "finished" => Ok(Self::Finished),
            "pending" => Ok(Self::Pending),
            "pending_passed" => Ok(Self::PendingPassed),
            _ => Err(v)
        }
    }

    fn default() -> Option<Self> {
        Some(Default::default())
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
        let csrf_token = match cookies.get("csrf_protection_token") {
            Some(token) => token.value().to_string(),
            None => {
                let new_token = generate_state(&mut rand::thread_rng()).unwrap();
                cookies.add(
                    Cookie::build("csrf_protection_token", new_token.clone())
                        .same_site(SameSite::Strict)
                        .secure(true)
                        .http_only(true)
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
                time datetime=(motion.end_at().to_rfc3339()) {
                    (motion.end_at().to_rfc2822())
                }
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
    html! {
        (maud::DOCTYPE)
        html {
            head {
                title { (title.as_ref()) }
                link rel="stylesheet" href="/main.css";
            }
            body {
                div.container {
                    @if let Some(deets) = ctx.deets.as_ref() {
                        @let item_types:Vec<String> = itdsl::item_types.select(itdsl::name).get_results(&**ctx).unwrap();
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
                        p { "Welcome, " (deets.discord_user.username) "#" (deets.discord_user.discriminator)}
                        form action="/logout" method="post" {
                            input type="hidden" name="csrf" value=(ctx.csrf_token);
                            input type="submit" name="submit" value="Logout";
                        }
                        ul {
                            @for (name, amount) in balances {
                                li { (amount) (name) }
                            }
                        }
                    } @else {
                        form action="/login/discord" method="post" {
                            input type="hidden" name="csrf" value=(ctx.csrf_token);
                            p { 
                                "I don't know who you are. You should"
                                input type="submit" name="submit" value="Login";
                            }
                        }
                    }
                    hr;
                    (content)
                }
            }
        }
    }
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
    if ctx.cookies.get("csrf_protection_token").map(|token| token.value()) != Some(data.csrf.as_str()) {
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
        (resp)
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
    let maybe_motion:Option<Motion> = mdsl::motions.select((
        mdsl::rowid,
        mdsl::bot_message_id,
        mdsl::motion_text,
        mdsl::motioned_at,
        mdsl::last_result_change,
        mdsl::is_super,
        mdsl::announcement_message_id,
    )).filter(mdsl::rowid.eq(id)).get_result(&*ctx).optional().unwrap();
    
    let motion;
    if let Some(m) = maybe_motion {
        motion = m;
    }else{
        return None;
    }

    let votes:Vec<MotionVote> = mvdsl::motion_votes
        .select((mvdsl::user, mvdsl::direction, mvdsl::amount))
        .filter(mvdsl::motion.eq(motion.rowid))
        .get_results(&*ctx)
        .unwrap();
    let (yes_vote_count, no_vote_count) = votes
        .iter()
        .map(|v| if v.direction { (v.amount, 0) } else { (0, v.amount) })
        .fold((0,0), |acc, x| (acc.0 + x.0, acc.1 + x.1));
    let motion = MotionWithCount::from_motion(motion, yes_vote_count as u64, no_vote_count as u64);
    let voting_html = if let Some(deets) = ctx.deets.as_ref() {
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
        html!{ "You must be logged in to vote." }
    };

    Some(page(&mut ctx, format!("Motion#{}", motion.damm_id()), html!{
        div.motion {
            a href="/" { "Home" }
            (motion_snippet(&motion))
            hr;
            (voting_html)
            hr;
            @for vote in &votes {
                div.motion-vote {
                    h5 { (vote.user) }
                    span {
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
    }))
}

#[get("/?<filter>")]
fn index(mut ctx: CommonContext, filter: MotionListFilter) -> impl Responder<'static> {
    use schema::motions::dsl as mdsl;
    use schema::motion_votes::dsl as mvdsl;
    let bare_motions:Vec<Motion> = mdsl::motions
        .select((
            mdsl::rowid,
            mdsl::bot_message_id,
            mdsl::motion_text,
            mdsl::motioned_at,
            mdsl::last_result_change,
            mdsl::is_super,
            mdsl::announcement_message_id,
        ))
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

    let all_motions = (bare_motions.into_iter().map(|m| {
        let yes_votes = get_vote_count(m.rowid, true)?;
        let no_votes = get_vote_count(m.rowid, false)?;
        Ok(MotionWithCount::from_motion(m, yes_votes as u64, no_votes as u64))
    }).collect():Result<Vec<_>,diesel::result::Error>).unwrap().into_iter();

    let motions = match filter {
        MotionListFilter::All => all_motions.collect(),

        MotionListFilter::Failed =>
            all_motions.filter(|m| m.announcement_message_id.is_some() && !m.is_win).collect(),

        MotionListFilter::Finished =>
            all_motions.filter(|m| m.announcement_message_id.is_some()).collect(),

        MotionListFilter::Passed =>
            all_motions.filter(|m| m.announcement_message_id.is_some() &&  m.is_win).collect(),

        MotionListFilter::Pending =>
            all_motions.filter(|m| m.announcement_message_id.is_none()).collect(),

        MotionListFilter::PendingPassed =>
            all_motions.filter(|m| m.announcement_message_id.is_none() ||  m.is_win).collect(),
    }:Vec<_>;

    page(&mut ctx, "All Motions", html!{
        form method="get" {
            div {
                "Filters:"
                ul {
                    @let options = [
                        ("all", "All", MotionListFilter::All),
                        ("passed", "Passed", MotionListFilter::Passed),
                        ("failed", "Failed", MotionListFilter::Failed),
                        ("finished", "Finished (Passed or Failed)", MotionListFilter::Finished),
                        ("pending", "Pending", MotionListFilter::Pending),
                        ("pending_passed", "Pending or Passed", MotionListFilter::PendingPassed),
                    ];
                    @for (codename, textname, val) in &options {
                        li {
                            label {
                                input type="radio" name="filter" value=(codename) checked?[filter == *val];
                                (textname)
                            }
                        }
                    }
                }
                input type="submit" name="submit" value="Go";
            }
        }
        @for motion in &motions {
            div.motion {
                (motion_snippet(&motion))
            }
        }
    })
}

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
    Ok(Redirect::to("/"))
}

#[post("/login/discord", data = "<data>")]
fn login(
    oauth2: OAuth2<DiscordOauth>,
    mut cookies: Cookies<'_>,
    data: LenientForm<CSRFForm>,
) -> Result<Redirect, rocket::http::Status> {
    if cookies.get("csrf_protection_token").map(|token| token.value()) != Some(data.csrf.as_str()) {
        return Err(rocket::http::Status::BadRequest);
    }
    Ok(oauth2.get_redirect(&mut cookies, &["identify"]).unwrap())
}

#[get("/motions")]
fn motions_api_compat(
    ctx: CommonContext
) -> impl Responder {
    use schema::motions::dsl as mdsl;
    use schema::motion_votes::dsl as mvdsl;
    let bare_motions:Vec<Motion> = mdsl::motions.select((
        mdsl::rowid,
        mdsl::bot_message_id,
        mdsl::motion_text,
        mdsl::motioned_at,
        mdsl::last_result_change,
        mdsl::is_super,
        mdsl::announcement_message_id,
    )).get_results(&*ctx).unwrap();

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

pub fn main() {
    rocket::ignite()
        .manage(rocket_diesel::init_pool())
        .attach(OAuth2::<DiscordOauth>::fairing("discord"))
        .mount("/", StaticFiles::from("static"))
        .mount("/",routes![
            index,
            oauth_finish,
            login,
            //cookies,
            get_deets,
            motion_listing,
            motion_vote,
            motions_api_compat
        ])
        .launch();
}