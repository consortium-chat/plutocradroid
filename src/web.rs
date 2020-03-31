use std::borrow::Cow;
use iron::prelude::*;
use iron::status;
use router::Router;
use diesel::prelude::*;
use chrono::{DateTime,Utc};

use crate::schema;
//use crate::view_schema;
use crate::iron_diesel::{DieselReqExt, DieselMiddleware};

use serde::Serialize;

//use schema::{motions, motion_votes};

#[derive(Clone,Debug,Serialize,Queryable)]
struct Motion<'a> {
    pub rowid:i64,
    pub bot_message_id:i64,
    pub motion_text:Cow<'a, str>,
    pub motioned_at:DateTime<Utc>,
    pub last_result_change:DateTime<Utc>,
    pub is_super:bool,
    pub announcement_message_id:Option<i64>,
}

#[derive(Clone,Debug,Serialize,Queryable)]
struct MotionVote {
    pub user:i64,
    pub direction:bool,
    pub amount:i64,
}

fn index(_: &mut Request) -> IronResult<Response> {
    Ok(Response::with((status::Ok, "API only\n")))
}

fn hello_world(_: &mut Request) -> IronResult<Response> {
    Ok(Response::with((status::Ok, "Hello World!")))
}

fn motions(req: &mut Request) -> IronResult<Response> {
    let conn = req.get_db_conn();
    use schema::motions::dsl as mdsl;
    let res:Vec<Motion> = mdsl::motions.select((
        mdsl::rowid,
        mdsl::bot_message_id,
        mdsl::motion_text,
        mdsl::motioned_at,
        mdsl::last_result_change,
        mdsl::is_super,
        mdsl::announcement_message_id,
    )).get_results(&*conn).unwrap();

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
    router.get("/motion_votes/:id", motion_votes, "motion_votes");
    //router.get("/:query", handler, "query");

    let mut chain = Chain::new(router);
    chain.link_before(DieselMiddleware::new());

    let listen_address = std::env::var("LISTEN_ADDRESS").unwrap();
    println!("Listening on {}", listen_address);
    Iron::new(chain).http(listen_address).unwrap();
}