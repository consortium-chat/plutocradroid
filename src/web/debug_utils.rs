use std::collections::HashMap;

use rocket::response::Redirect;
use rocket::http::{Cookie,Cookies,SameSite};

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

#[get("/debug_util/fabricate?<user>&<ty>&<amt>")]
pub fn fabricate(
    ctx: CommonContext,
    user: i64,
    ty: String,
    amt: i64,
) -> Result<Redirect, template::ErrorResponse> {
    let now = Utc::now();
    if user <= 0 || amt < 0 {
        return hard_err(rocket::http::Status::BadRequest);
    }
    use schema::item_types::dsl as itdsl;
    let maybe_ty:Option<models::ItemType> = itdsl::item_types
        .select(models::ItemType::cols())
        .filter(itdsl::name.eq(ty.as_str()))
        .get_result(&*ctx)
        .optional()
        .unwrap();
    
    let ty = match maybe_ty {
        Some(v) => v,
        None => return hard_err(rocket::http::Status::BadRequest),
    };

    let t = TransactionBuilder::new(amt, ty.id, now).fabricate(user.try_into().unwrap(), false);
    ctx.conn.transaction::<_, diesel::result::Error, _>(|| TransferHandler::handle_single(&*ctx, t).unwrap()).unwrap();

    Ok(Redirect::to("/debug_util"))
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
            "current context:"
            br;
            pre {
                (ctx_debug)
            }
        },
    )
}