use rocket::http::{Cookies,Cookie};
use rocket::request::{FromRequest, Request, Outcome};

use super::deets::{Deets, DeetsFail};
use super::csrf::CSRF_COOKIE_NAME;

#[derive(Debug)]
pub enum CommonContextError {
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
pub struct CommonContext<'a> {
    pub csrf_token: String,
    pub cookies: Cookies<'a>,
    pub deets: Option<Deets>,
    pub conn: super::rocket_diesel::DbConn,
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
                let new_token = super::csrf::generate_state(&mut rand::thread_rng()).unwrap();
                cookies.add(
                    Cookie::build(CSRF_COOKIE_NAME, new_token.clone())
                        .same_site(rocket::http::SameSite::Lax)
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

        let conn = super::rocket_diesel::DbConn::from_request(request).map_failure(|(a,_)| (a, CommonContextError::from(())))?;
        Outcome::Success(Self{
            csrf_token,
            cookies,
            deets,
            conn,
        })
    }
}