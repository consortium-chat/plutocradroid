use rocket::response::Redirect;

use super::prelude::*;

/// Intended for nginx to internally redirect from shortlink domains, but if someone goes here directly that's fine
#[get("/shortlink/<damm_id>")]
pub fn shortlink(
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
        return Some(Redirect::permanent(full_url(uri!(super::motions::motion_view: damm_id = damm_id))));
    }

    let is_auction = select(exists(adsl::auctions.filter(adsl::rowid.eq(id)))).get_result(&*ctx).unwrap();
    if is_auction {
        return Some(Redirect::permanent(full_url(uri!(super::auctions::auction_view:   damm_id = damm_id))));
    }

    None
}