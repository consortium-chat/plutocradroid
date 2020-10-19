use iron::prelude::*;
use iron::status;
use router::Router;
use diesel::prelude::*;

use crate::schema;
//use crate::view_schema;
use crate::iron_diesel::{DieselReqExt, DieselMiddleware};

use crate::models::{Motion, MotionVote, MotionWithCount};


fn index(_: &mut Request) -> IronResult<Response> {
    Ok(Response::with((status::Ok, "API only\n")))
}

fn hello_world(_: &mut Request) -> IronResult<Response> {
    Ok(Response::with((status::Ok, "Hello World!")))
}

fn motions(req: &mut Request) -> IronResult<Response> {
    let conn = req.get_db_conn();
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
    )).get_results(&*conn).unwrap();

    let get_vote_count = |motion_id:i64, dir:bool| -> Result<i64, diesel::result::Error> {
        use bigdecimal::{BigDecimal,ToPrimitive};
        let votes:Option<BigDecimal> = mvdsl::motion_votes
        .select(diesel::dsl::sum(mvdsl::amount))
        .filter(mvdsl::motion.eq(motion_id))
        .filter(mvdsl::direction.eq(dir))
        .get_result(&*conn)?;
        Ok(votes.map(|bd| bd.to_i64().unwrap()).unwrap_or(0))
    };

    let res = (bare_motions.into_iter().map(|m| {
        let yes_votes = get_vote_count(m.rowid, true)?;
        let no_votes = get_vote_count(m.rowid, false)?;
        Ok(MotionWithCount::from_motion(m, yes_votes as u64, no_votes as u64))
    }).collect():Result<Vec<_>,diesel::result::Error>).unwrap();

    Ok(Response::with((status::Ok, serde_json::to_string(&res).unwrap())))
}

fn motion_votes(req: &mut Request) -> IronResult<Response> {
    let id:i64 = req.extensions
        .get::<router::Router>()
        .unwrap()
        .find("id")
        .unwrap()
        .parse()
        .map_err(|e| 
            IronError::new(e, (status::BadRequest, "Invalid number.",))
        )?;
    let conn = req.get_db_conn();
    use schema::motion_votes::dsl as mvdsl;
    let res:Vec<MotionVote> = mvdsl::motion_votes.select((
        mvdsl::user,
        mvdsl::direction,
        mvdsl::amount,
    )).filter(mvdsl::motion.eq(id)).get_results(&*conn).unwrap();

    Ok(Response::with((status::Ok, serde_json::to_string(&res).unwrap())))
}

pub fn web_main() {
    let mut router = Router::new();
    router.get("/", index, "index");
    router.get("/hello", hello_world, "hello world");
    router.get("/motions", motions, "motions");
    router.get("/votes/:id", motion_votes, "motion_votes");
    //router.get("/:query", handler, "query");

    let mut chain = Chain::new(router);
    chain.link_before(DieselMiddleware::new());

    let listen_address = std::env::var("LISTEN_ADDRESS").unwrap();
    println!("Listening on {}", listen_address);
    Iron::new(chain).http(listen_address).unwrap();
}
