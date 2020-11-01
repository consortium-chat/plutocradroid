use rocket::request::Request;
use rocket::response::{Responder, Response};
use rocket::http::Status;
use hyper::header::Header;
use rocket::http::hyper::header::{
    Expires,
    HttpDate, 
    ETag,
    EntityTag,
    CacheControl,
    CacheDirective,
    IfNoneMatch,
};
use time::{self, Duration};

#[derive(Debug, Clone, PartialEq)]
pub struct LongLived<R>(pub R);

impl<'r, R: Responder<'r>> Responder<'r> for LongLived<R> {
    fn respond_to(self, req: &Request) -> Result<Response<'r>, Status> {
        Response::build()
            .merge(self.0.respond_to(req)?)
            .header(Expires(HttpDate(time::now() + Duration::weeks(53))))
            .header(CacheControl(vec![
                CacheDirective::Public,
                CacheDirective::MaxAge(604800),
                CacheDirective::Extension(String::from("immutable"),None),
            ]))
            .ok()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tagged<R>(pub String, pub R);

impl<'r, R: Responder<'r>> Responder<'r> for Tagged<R> {
    fn respond_to(self, req: &Request) -> Result<Response<'r>, Status> {
        let tag = EntityTag::new(false, self.0);
        if let Some(Ok(header)) = req.headers().get_one("if-none-match").map(|h| <IfNoneMatch as Header>::parse_header(&[h.as_bytes().into()])){
            let is_match = match header {
                IfNoneMatch::Any => true,
                IfNoneMatch::Items(items) => items.iter().any(|i| i.strong_eq(&tag)),
            };
            if is_match {
                return Response::build()
                    .status(Status::NotModified)
                    .header(Expires(HttpDate(time::now() + Duration::days(1))))
                    .header(ETag(tag))
                    .ok();
            }
        }
        Response::build()
            .merge(self.1.respond_to(req)?)
            .header(Expires(HttpDate(time::now() + Duration::days(1))))
            .header(ETag(tag))
            .ok()
    }
}
