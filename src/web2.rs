
#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}
pub fn main() {
    rocket::ignite().mount("/",routes![index]).launch();
}