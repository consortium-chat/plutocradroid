use rocket::fairing;
use rocket::request::Request;
use rocket::response::Response;

#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub struct SecureHeaders;

impl fairing::Fairing for SecureHeaders {
    fn info(&self) -> fairing::Info {
        fairing::Info {
            name: "Secure Headers Fairing",
            kind: fairing::Kind::Response,
        }
    }

    fn on_response(&self, _request: &Request, response: &mut Response) {
        use rocket::http::Header;
        response.adjoin_header(Header::new(
            "Content-Security-Policy",
            "default-src 'none'; frame-ancestors 'none'; img-src 'self'; script-src 'self'; style-src 'self'"
        ));
        response.adjoin_header(Header::new(
            "Referrer-Policy",
            "strict-origin-when-cross-origin"
        ));
        response.adjoin_header(Header::new(
            "X-Content-Type-Options",
            "nosniff"
        ));
        response.adjoin_header(Header::new(
            "X-Frame-Options",
            "DENY"
        ));
        // Strict-Transport-Security is purposefully omitted here; Rocket does not support SSL/TLS. The layer that is adding SSL/TLS (most likely nginx or apache) should add an appropriate STS header.
    }
}
