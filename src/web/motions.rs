use std::collections::HashMap;
use std::borrow::Cow;
use rocket::request::{FromQuery, Query};

use super::prelude::*;
use crate::models::{Motion,MotionWithCount,MotionVote,Transfer,TransferExtra};
use crate::motion_label::motion_label;

#[derive(Debug, Clone, FromForm)]
pub struct VoteForm {
    csrf: String,
    count: i64,
    direction: String,
}

#[derive(Debug,Copy,Clone,PartialEq,Eq)]
pub struct MotionFilter {
    pub pending: bool,
    pub passed: bool,
    pub failed: bool,
}

impl MotionFilter {
    pub fn as_query(&self) -> Option<String> {
        if self.pending && self.passed && self.failed {
            return None;
        }
        let mut res = "?filter=y".to_string();
        if self.pending { res.push_str("&pending=y") }
        if self.passed { res.push_str("&passed=y") }
        if self.failed { res.push_str("&failed=y") }
        Some(res)
    }
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

impl rocket::http::uri::Ignorable<rocket::http::uri::Query> for MotionFilter {}

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

fn motion_meta_description(
    motion: &crate::models::MotionWithCount,
    detailed: bool,
) -> String {
    let votes = if motion.is_win {
        format!(
            "{} IN FAVOR vs {} against",
            motion.yes_vote_count,
            motion.no_vote_count,
        )
    } else {
        format!(
            "{} AGAINST vs {} in favor",
            motion.yes_vote_count,
            motion.no_vote_count,
        )
    };

    let motion_text = format!(
        "{} {}",
        motion_label(&motion.power),
        motion.motion_text
    );

    let result = if motion.announcement_message_id.is_some() {
        if motion.is_win {
            "PASSED"
        } else {
            "FAILED"
        }.to_string()
    } else {
        format!(
            "May {}",
            if motion.is_win {
                "pass"
            } else {
                "fail"
            }
        )
    };

    if detailed || motion.announcement_message_id.is_some() {
        format!(
            "{} with {} at {}: {}",
            result,
            votes,
            ts_plain(motion.end_at()),
            motion_text,
        )
    } else {
        motion_text
    }
}

#[allow(clippy::branches_sharing_code)]
fn motion_snippet(
    motion: &crate::models::MotionWithCount
) -> maud::Markup {
    let cap_label = motion_label(&motion.power);
    maud::html!{
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
                (super::template::show_ts(motion.end_at()))
            }
        }
        p {
            (cap_label)
            " "
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

#[get("/motions?<filters..>")]
pub fn motion_index(
    mut ctx: CommonContext,
    filters: MotionFilter,
) -> PlutoResponse {

    use crate::schema::motions::dsl as mdsl;
    use crate::schema::motion_votes::dsl as mvdsl;
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

    page(
        &mut ctx,
        PageTitle("Motions".to_string()),
        CanonicalUrl(Some(format!(
            "{}/motions{}",
            SITE_URL,
            filters.as_query().unwrap_or_default(),
        ))),
        html!{
            //head content
        },
        html!{
            h1 { "Motions" }
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
        }
    )
}

#[get("/motions/<damm_id>?<cb>")]
pub fn motion_view(
    mut ctx: CommonContext,
    damm_id: String,
    cb: Option<String>, //cache buster
) -> PlutoResponse {
    let id:i64 = if let Some(digits) = crate::damm::validate_ascii(damm_id.as_str()) {
        atoi::atoi(digits.as_slice()).unwrap()
    } else {
        return not_found();
    };

    use schema::motions::dsl as mdsl;
    use schema::motion_votes::dsl as mvdsl;
    use schema::transfers::dsl as tdsl;
    let maybe_motion:Option<Motion> = mdsl::motions
        .select(Motion::cols())
        .filter(mdsl::rowid.eq(id))
        .get_result(&*ctx)
        .optional()
        .unwrap();
    
    let motion = if let Some(m) = maybe_motion {
        m
    }else{
        return not_found();
    };

    let votes:Vec<MotionVote> = mvdsl::motion_votes
        .select(MotionVote::cols())
        .filter(mvdsl::motion.eq(motion.rowid))
        .get_results(&*ctx)
        .unwrap();
    let (yes_vote_count, no_vote_count) = votes
        .iter()
        .map(|v| if v.direction { (v.amount, 0) } else { (0, v.amount) })
        .fold((0,0), |acc, x| (acc.0 + x.0, acc.1 + x.1));
    let vote_directions:HashMap<crate::models::UserId, bool> = votes
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
                if vote.user == deets.id() {
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

    let mut motion_history:Vec<(DateTime<Utc>, Cow<'static, str>,String)> = vec![];

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
                                "in favor of"
                            } else {
                                "against"
                            },
                            votes
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
                        dt { (name_of(vote.user.into_serenity())) }
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

    let meta_title = format!("Motion #{}", motion.damm_id());
    
    let meta_description = motion_meta_description(&motion, cb.is_some());

    let self_uri = full_url(uri!(motion_view: damm_id = &damm_id, cb = _));

    page(
        &mut ctx,
        PageTitle(format!("Motion #{}", motion.damm_id())),
        self_uri.clone().into(),
        html!{
            (embed_head_html(meta_title, meta_description, &self_uri))

            link rel="index" href=(uri!(motion_index: _));
        },
        markup
    )
}

#[post("/motions/<damm_id>/vote", data = "<data>")]
pub fn motion_vote(
    mut ctx: CommonContext,
    data: LenientForm<VoteForm>,
    damm_id: String,
) -> PlutoResponse {
    let id = if let Some(digits) = crate::damm::validate_ascii(damm_id.as_str()) {
        atoi::atoi(digits.as_slice()).unwrap()
    } else {
        info!("bad id");
        return not_found();
    };
    if ctx.cookies.get(CSRF_COOKIE_NAME).map(|token| token.value()) != Some(data.csrf.as_str()) {
        return hard_err(Status::BadRequest);
    }
    let deets:&Deets = if let Some(d) = ctx.deets.as_ref() {
        d
    } else {
        info!("no deets");
        return hard_err(Status::Unauthorized);
    };
    let vote_count = data.count;
    let vote_direction:bool;
    if data.direction.as_str() == "for" {
        vote_direction = true;
    } else if data.direction.as_str() == "against" {
        vote_direction = false;
    } else {
        info!("bad vote direction {:?}", data.direction);
        return hard_err(rocket::http::Status::BadRequest);
    }
    let resp = crate::bot::vote_common(
        &ctx.conn, 
        Some(vote_direction),
        vote_count,
        deets.id(),
        Some(id),
        None,
        None
    );

    page(
        &mut ctx,
        PageTitle("Vote Complete".to_string()),
        CanonicalUrl(None),
        html!{},
        html!{
            main { (resp) }
            br;
            a href={"/motions/" (damm_id)} { "Back to Motion" }
            br;
            a href="/" { "Back Home" }
        }
    )
}