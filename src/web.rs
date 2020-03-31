use iron::prelude::*;
use iron::status;
use router::Router;

fn index(_: &mut Request) -> IronResult<Response> {
    Ok(Response::new())
}

pub fn web_main() {
    let mut router = Router::new();
    router.get("/", index, "index");
    //router.get("/:query", handler, "query");

    let listen_address = std::env::var("LISTEN_ADDRESS").unwrap();
    println!("Listening on {}", listen_address);
    Iron::new(router).http(listen_address).unwrap();


    fn hello_world(_: &mut Request) -> IronResult<Response> {
        Ok(Response::with((status::Ok, "Hello World!")))
    }
}