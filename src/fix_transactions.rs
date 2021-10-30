use std::collections::HashMap;
use diesel::prelude::*;
use schema::transfers::dsl as tdsl;

#[derive(Debug,Clone,ParitalEq,Queryable)]
struct Transfer {
    rowid: i64,
    ty: String,
    from_user: Option<i64>,
    quantity: i64,
    to_user: Option<i64>,
    from_balance: Option<i64>,
    to_balance: Option<i64>,
}

#[derive(Debug,Clone)]
struct BalPair<'a> {
    user: i64,
    balance: i64,
}

impl Transfer {
    fn cols() -> (
        tdsl::rowid,
        tdsl::ty,
        tdsl::from_user,
        tdsl::quantity,
        tdsl::to_user,
        tdsl::from_balance,
        tdsl::to_balance
    ) {
        (
            tdsl::rowid,
            tdsl::ty,
            tdsl::from_user,
            tdsl::quantity,
            tdsl::to_user,
            tdsl::from_balance,
            tdsl::to_balance
        )
    }

    fn from(&self) -> Option<BalPair> {
        self.from_user.map(|u| BalPair{user: u, balance: self.from_balance.unwrap()})
    }

    fn to(&self) -> Option<BalPair> {
        self.to_user.map(|u| BalPair{user: u, balance: self.to_balance.unwrap()})
    }

    fn balpairs(&self) -> Vec<BalPair> {
        let mut res = Vec::new();
        if let Some(a) = self.from() { res.push(a) }
        if let Some(a) = self.to() { res.push(a) }
        res
    }
}

pub fn fix_transactions() {
    let mut fail = false;
    let conn = diesel::PgConnection::establish(
        &std::env::var("DATABASE_URL").expect("DATABASE_URL expected")
    ).unwrap();

    conn.transaction::<_, diesel::result::Error, _>(|| {
        let transfers:Vec<Transfer> = tdsl::transfers
        .select(Transfer::cols())
        .order_by(tdsl::happened_at.asc())
        .load(&conn)
        .unwrap();
        let balances = HashMap::new();
        for orig_transfer in transfers {
            let mut new_transfer = orig_transfer.clone();
            if let Some(bp) = orig_transfer.from() {
                let prev_balance = balances.entry((orig_transfer.ty.clone(), bp.user)).or_insert(0i64);
                let new_balance = prev_balance - orig_transfer.quantity;
                if new_balance < 0 { fail = true }
                new_transfer.from_balance = Some(new_balance);
                balances.insert((orig_transfer.ty.clone(), bp.user), new_balance);
            }
            if let Some(bp) = orig_transfer.to() {
                let prev_balance = balances.entry((orig_transfer.ty.clone(), bp.user)).or_insert(0i64);
                let new_balance = prev_balance + orig_transfer.quantity;
                new_transfer.to_balance = Some(new_balance);
                balances.insert((orig_transfer.ty.clone(), bp.user), new_balance);
            }
            if new_transfer != orig_transfer {
                diesel::update(tdsl::transfers.filter(tdsl::rowid.eq(new_transfer.rowid)))
                .set((
                    tdsl::from_balance.eq(new_transfer.from_balance()),
                    tdsl::to_balance.eq(new_transfer.to_balance()),
                ))
                .execute()
                .unwrap()
            }
        }

        if fail {
            Err(diesel::result::Error::RollbackTransaction)
        } else {
            Ok(())
        }
    }).unwrap();

    // NO KNOWLEDGE ALLOWED

    let mut rng = rand::thread_rng();
    let y:f64 = rng.gen();
    std::thread::sleep(std::time::Duration::from_secs_f64(y * 100));
    if fail {
        println!("Failed");
    } else {
        println!("Success");
    }
}