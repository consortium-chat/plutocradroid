use std::collections::HashMap;

use rocket::response::Redirect;
use rocket::http::{Cookie,Cookies,SameSite,Status};

use crate::models;
use super::prelude::*;
use super::template;
use super::deets::DiscordUser;

lazy_static::lazy_static!{
    static ref DEBUG_USERS:HashMap<i64,Deets> = maplit::hashmap!{
        2 => Deets{
            discord_user: DiscordUser{
                id: "2".to_string(),
                username: "Dos".to_string(),
                discriminator: "2222".to_string(),
                avatar: "".to_string(),
            }
        },
        3 => Deets{
            discord_user: DiscordUser{
                id: "3".to_string(),
                username: "Three".to_string(),
                discriminator: "3333".to_string(),
                avatar: "".to_string(),
            }
        },
        4 => Deets{
            discord_user: DiscordUser{
                id: "4".to_string(),
                username: "Four".to_string(),
                discriminator: "4444".to_string(),
                avatar: "".to_string(),
            }
        },
    };
}

#[get("/debug_util/impersonate?<user>")]
pub fn impersonate(
    mut cookies: Cookies<'_>,
    user: i64,
) -> Result<Redirect, template::ErrorResponse> {
    let new_deets = match DEBUG_USERS.get(&user) {
        None => return hard_err(Status::BadRequest),
        Some(v) => v,
    };
    cookies.add_private(
        Cookie::build("deets", serde_json::to_string(&new_deets).unwrap())
            .same_site(SameSite::Lax)
            .secure(true)
            .http_only(true)
            .finish()
    );

    Ok(Redirect::to("/debug_util"))
}

fn get_ty(
    conn: &diesel::pg::PgConnection,
    ty: String,
) -> Result<models::ItemType, template::ErrorResponse> {
    use schema::item_types::dsl as itdsl;

    let maybe_ty:Option<models::ItemType> = itdsl::item_types
        .select(models::ItemType::cols())
        .filter(itdsl::name.eq(ty.as_str()))
        .get_result(conn)
        .optional()
        .unwrap();
    
    let ty = match maybe_ty {
        Some(v) => v,
        None => return hard_err(Status::BadRequest),
    };
    Ok(ty)
}

#[get("/debug_util/fabricate?<user>&<ty>&<amt>")]
pub fn fabricate(
    ctx: CommonContext,
    user: i64,
    ty: String,
    amt: i64,
) -> Result<Redirect, template::ErrorResponse> {
    let now = Utc::now();
    if user <= 0 || amt < 0 {
        return hard_err(Status::BadRequest);
    }
    
    let ty = get_ty(&*ctx, ty)?;

    let t = TransactionBuilder::new(amt, ty.id, now).fabricate(user.try_into().unwrap(), false);
    ctx.conn.transaction::<_, diesel::result::Error, _>(|| TransferHandler::handle_single(&*ctx, t).unwrap()).unwrap();

    Ok(Redirect::to("/debug_util"))
}

#[get("/debug_util/make_auction?<offer_ty>&<bid_ty>&<amt>&<min>")]
pub fn make_auction(
    ctx: CommonContext,
    offer_ty: String,
    bid_ty: String,
    amt: i64,
    min: i64,
) -> Result<Redirect, template::ErrorResponse> {
    let now = Utc::now();
    if amt < 1 {
        return hard_err(Status::BadRequest);
    }
    
    let offer_ty = get_ty(&*ctx, offer_ty)?;
    let bid_ty = get_ty(&*ctx, bid_ty)?;

    use schema::auctions::dsl as adsl;
    use schema::thing_ids::dsl as tdsl;
    let id:i64 = diesel::insert_into(tdsl::thing_ids)
        .default_values()
        .returning(tdsl::rowid)
        .get_result(&*ctx)
        .unwrap();
    diesel::insert_into(adsl::auctions).values((
        adsl::rowid.eq(id),
        adsl::created_at.eq(now),
        adsl::auctioneer.eq(None:Option<i64>),
        adsl::offer_ty.eq(offer_ty.id),
        adsl::offer_amt.eq(amt),
        adsl::bid_ty.eq(bid_ty.id),
        adsl::bid_min.eq(min),
        adsl::finished.eq(false),
        adsl::last_timer_bump.eq(now),
    )).execute(&*ctx).unwrap();

    let uri = uri!(super::auctions::auction_view: damm_id = crate::damm::add_to_str(id.to_string()), cb = _);

    Ok(Redirect::to(uri))
}

#[get("/debug_util/make_motion?<is_super>&<content>&<called_by>")]
pub fn make_motion(
    ctx: CommonContext,
    is_super: bool,
    content: String,
    called_by: i64,
) -> Result<Redirect, template::ErrorResponse> {
    let now = Utc::now();
    if called_by < 0 {
        return hard_err(Status::BadRequest);
    }

    use schema::motions::dsl as mdsl;
    use schema::thing_ids::dsl as tdsl;

    let id:i64 = diesel::insert_into(tdsl::thing_ids)
        .default_values()
        .returning(tdsl::rowid)
        .get_result(&*ctx)
        .unwrap();
    diesel::insert_into(mdsl::motions).values((
        mdsl::rowid.eq(id),
        mdsl::command_message_id.eq(-1), //HAXXXX
        mdsl::bot_message_id.eq(-1), //MOAR HAXXXX
        mdsl::motion_text.eq(content),
        mdsl::motioned_at.eq(now),
        mdsl::last_result_change.eq(now),
        mdsl::is_super.eq(is_super),
        mdsl::motioned_by.eq(called_by),
    )).execute(&*ctx).unwrap();

    let uri = uri!(super::motions::motion_view: damm_id = crate::damm::add_to_str(id.to_string()), cb = _);

    Ok(Redirect::to(uri))
}

#[get("/debug_util")]
pub fn debug_util_forms(
    mut ctx: CommonContext,
) -> Result<template::OkResponse, !> {
    let ctx_debug: String = format!("{ctx:#?}");
    page(
        &mut ctx,
        PageTitle("DEBUG TOOLS"),
        CanonicalUrl(None),
        html!{},
        html!{
            "Impersonate"
            form action="/debug_util/impersonate" method="get" {
                select name="user" {
                    @for k in DEBUG_USERS.keys() {
                        option value=(k) { (k) }
                    }
                }
                button type="submit" { "go" }
            }
            br;br;
            "Fabricate"
            form action="/debug_util/fabricate" method="get" {
                input type="number" name="amt";
                input type="text" name="ty";
                " to "
                input type="number" name="user";
                button type="submit" { "go" }
            }
            br;br;
            "Make auction for"
            form action="/debug_util/make_auction" method="get" {
                input type="number" name="amt";
                input type="text" name="offer_ty";
                "for at least"
                input type="number" name="min";
                input type="text" name="bid_ty";
                button type="submit" { "go" }
            }
            br;br;
            form action="/debug_util/make_motion" method="get" {
                "Make "
                select name="is_super" {
                    option value="false" { "simple" }
                    option value="true" { "super" }
                }
                " motion called by "
                input type="number" name="called_by";
                br;
                textarea name="content" {}
                br;
                button type="submit" { "go" }
            }
            br;br;
            "current context:"
            br;
            pre {
                (ctx_debug)
            }
        },
    )
}