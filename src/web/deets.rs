use rocket::request::{FromRequest, Request, Outcome};

#[derive(Deserialize,Serialize,Debug,Clone)]
pub struct DiscordUser {
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
pub struct Deets {
    pub discord_user: DiscordUser,
}

impl Deets {
    pub fn id(&self) -> crate::models::UserId {
        self.discord_user.id().try_into().unwrap()
    }
}

#[derive(Debug)]
pub enum DeetsFail {
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