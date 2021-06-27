use diesel::Connection;
use diesel::prelude::*;

pub struct RaiiTransaction<'a, C: Connection> {
    conn: &'a mut C,
    finished: bool,
}

impl<'a, C: Connection> RaiiTransaction<'a, C> {
    fn new(conn: &'a mut C) -> Result<Self,diesel::result::Error> {
        diesel::sql_query("start transaction").execute(conn)?;
        Ok(Self{
            conn,
            finished: false,
        })
    }

    pub fn commit(mut self) -> Result<(), diesel::result::Error> {
        diesel::sql_query("commit").execute(self.conn)?;
        self.finished = true;
        Ok(())
    }

    pub fn rollback(mut self) -> Result<(), diesel::result::Error> {
        diesel::sql_query("rollback").execute(self.conn)?;
        self.finished = true;
        Ok(())
    }
}

impl<'a, C: Connection> Drop for RaiiTransaction<'a, C> {
    fn drop(&mut self) {
        if !self.finished {
            let _ = diesel::sql_query("rollback").execute(self.conn);
        }
    }
}

impl<'a, C: Connection> std::ops::Deref for RaiiTransaction<'a, C> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

pub trait ConnectionExt: Connection + Sized {
    fn raii_transaction<'a>(&'a mut self) -> Result<RaiiTransaction<'a, Self>, diesel::result::Error>;
}

impl<T: Connection + Sized> ConnectionExt for T {
    fn raii_transaction<'a>(&'a mut self) -> Result<RaiiTransaction<'a, Self>, diesel::result::Error> {
        RaiiTransaction::new(self)
    }
}