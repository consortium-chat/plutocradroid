use std::collections::HashMap;
use diesel::prelude::*;
use rand::Rng;
use crate::schema::transfers::dsl as tdsl;

#[derive(Debug,Clone,PartialEq,Queryable)]
struct Transfer {
    rowid: i64,
    ty: String,
    from_user: Option<i64>,
    quantity: i64,
    to_user: Option<i64>,
    from_balance: Option<i64>,
    to_balance: Option<i64>,
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
}

pub fn fix_transactions() {
    let mut fail = false;
    let conn = diesel::PgConnection::establish(
        &std::env::var("DATABASE_URL").expect("DATABASE_URL expected")
    ).unwrap();

    let res = conn.transaction::<_, diesel::result::Error, _>(|| {
        let transfers:Vec<Transfer> = tdsl::transfers
        .select(Transfer::cols())
        .order_by(tdsl::happened_at.asc())
        .load(&conn)
        .unwrap();
        let mut balances = HashMap::new();
        for orig_transfer in transfers {
            let mut new_transfer = orig_transfer.clone();
            if let Some(user) = orig_transfer.from_user {
                let prev_balance = balances.entry((orig_transfer.ty.clone(), user)).or_insert(0i64);
                let new_balance = *prev_balance - orig_transfer.quantity;
                if new_balance < 0 { fail = true }
                new_transfer.from_balance = Some(new_balance);
                balances.insert((orig_transfer.ty.clone(), user), new_balance);
            }
            if let Some(user) = orig_transfer.to_user {
                let prev_balance = balances.entry((orig_transfer.ty.clone(), user)).or_insert(0i64);
                let new_balance = *prev_balance + orig_transfer.quantity;
                new_transfer.to_balance = Some(new_balance);
                balances.insert((orig_transfer.ty.clone(), user), new_balance);
            }
            if new_transfer != orig_transfer {
                diesel::update(tdsl::transfers.filter(tdsl::rowid.eq(new_transfer.rowid)))
                .set((
                    tdsl::from_balance.eq(new_transfer.from_balance),
                    tdsl::to_balance.eq(new_transfer.to_balance),
                ))
                .execute(&conn)
                .unwrap();
            }
        }

        if fail {
            Err(diesel::result::Error::RollbackTransaction)
        } else {
            Ok(())
        }
    });

    match res {
        Ok(_) => (),
        Err(diesel::result::Error::RollbackTransaction) => (),
        e => e.unwrap(),
    }

    // NO KNOWLEDGE ALLOWED

    let mut rng = rand::thread_rng();
    let y:f64 = rng.gen();
    std::thread::sleep(std::time::Duration::from_secs_f64(y * 100.0));
    if fail {
        println!("Failed");
    } else {
        println!("Success");
    }
}