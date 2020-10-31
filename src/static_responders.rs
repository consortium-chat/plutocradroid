use rocket::request::Request;
use rocket::response::{Responder, Response};
use rocket::http::Status;

use rocket::http::hyper::header::{Expires, HttpDate, ETag, EntityTag};
use time::{self, Duration};

#[derive(Debug, Clone, PartialEq)]
pub struct LongLived<R>(pub R);

impl<'r, R: Responder<'r>> Responder<'r> for LongLived<R> {
    fn respond_to(self, req: &Request) -> Result<Response<'r>, Status> {
        Response::build()
            .merge(self.0.respond_to(req)?)
            .header(Expires(HttpDate((time::now() + Duration::weeks(53)).into())))
            .ok()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tagged<R>(pub String, pub R);

impl<'r, R: Responder<'r>> Responder<'r> for Tagged<R> {
    fn respond_to(self, req: &Request) -> Result<Response<'r>, Status> {
        Response::build()
            .merge(self.1.respond_to(req)?)
            .header(Expires(HttpDate(time::now() + Duration::days(1))))
            .header(ETag(EntityTag::new(false, self.0)))
            .ok()
    }
}
