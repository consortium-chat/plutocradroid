//based on https://github.com/DavidBM/rust-webserver-example-with-iron-diesel-r2d2-serde/blob/49bc24b56d4644ffa3d8c97354836014d66bcfb6/src/middlewares/diesel_pool.rs
use diesel::PgConnection;
use diesel::r2d2;
use diesel::r2d2::ConnectionManager;
use iron::{typemap, BeforeMiddleware, Request, IronResult};
use std::sync::Arc;
use std::env;

pub type DieselConnection = r2d2::PooledConnection<ConnectionManager<PgConnection>>;
pub type DieselPool = r2d2::Pool<ConnectionManager<PgConnection>>;

pub struct DieselMiddleware {
	pool: Arc<DieselPool>
}

impl DieselMiddleware {
	pub fn new() -> DieselMiddleware{
		let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

		let manager = ConnectionManager::<PgConnection>::new(database_url);
		let pool = r2d2::Pool::new(manager).expect("Failed to create diesel pool.");

		DieselMiddleware {pool: Arc::new(pool)}
	}
}

impl typemap::Key for DieselMiddleware { type Value = Arc<DieselPool>; }

impl BeforeMiddleware for DieselMiddleware {
	fn before(&self, req: &mut Request) -> IronResult<()> {
		req.extensions.insert::<DieselMiddleware>(self.pool.clone());
		Ok(())
	}
}

pub trait DieselReqExt {
	fn get_db_conn(&self) -> DieselConnection;
}

impl <'a, 'b>DieselReqExt for Request <'a, 'b> {
	fn get_db_conn(&self) -> DieselConnection {
		let pool = self.extensions.get::<DieselMiddleware>().unwrap();

		return pool.get().expect("Failed to get a db connection");
	}
}