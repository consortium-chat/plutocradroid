use rocket::request::{Request,FromRequest,Outcome};

#[derive(Debug)]
pub struct Referer<'a>(pub &'a str);

impl<'a, 'r> FromRequest<'a, 'r> for Referer<'a> {
    type Error = ();

    fn from_request(request: &'a Request<'r>) -> Outcome<Self, Self::Error> {
        match request.headers().get_one("Referer") {
            None => Outcome::Failure((rocket::http::Status::BadRequest,())),
            Some(val) => Outcome::Success(Referer(val)),
        }
    }
}