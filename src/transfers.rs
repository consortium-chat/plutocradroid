use std::collections::HashMap;
use std::borrow::Cow;
use std::convert::TryInto;
use chrono::{DateTime,Utc};
use diesel::sql_types::Text;
use diesel::backend::Backend;
use diesel::deserialize;
use diesel::serialize;
use diesel::prelude::*;
use crate::schema::transfers::dsl as tdsl;
use crate::view_schema::balance_history::dsl as bhdsl;
use crate::models::{UserId,TransferType,AuctionWinner};

// // 4 because all the current types of fungibles (pc, gen, sb)
// // are under 4 characters
// type CurrencyIdStr = SmallString<[u8; 4]>;

type CurrencyIdStr = Cow<'static, str>;

#[derive(Debug,Clone,PartialEq,Eq,PartialOrd,Ord,Hash,FromSqlRow,AsExpression)]
#[sql_type = "Text"]
pub struct CurrencyId(CurrencyIdStr);

impl std::fmt::Display for CurrencyId {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// pub mod currencies {
//     use super::CurrencyId;

//     lazy_static! {
//         pub static ref PC :CurrencyId = CurrencyId("pc".into());
//         pub static ref GEN:CurrencyId = CurrencyId("gen".into());
//     }
// }

impl CurrencyId {
    pub const PC :Self = Self(Cow::Borrowed("pc"));
    pub const GEN:Self = Self(Cow::Borrowed("gen"));
    pub fn as_str(&self) -> &str {
        match self.0 {
            Cow::Borrowed(v) => v,
            Cow::Owned(ref v) => v.as_str(),
        }
    }
}

impl AsRef<str> for CurrencyId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<DB> deserialize::FromSql<Text, DB> for CurrencyId
where
    DB: Backend,
    String: deserialize::FromSql<Text, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        Ok(CurrencyId(CurrencyIdStr::from(String::from_sql(bytes)?)))
    }
}

impl<DB> serialize::ToSql<Text, DB> for CurrencyId
where
    DB: Backend,
    str: serialize::ToSql<Text, DB>,
{
    fn to_sql<W: std::io::Write>(&self, out: &mut serialize::Output<W, DB>) -> serialize::Result {
        self.as_str().to_sql(out)
    }
}

pub struct TransactionBuilder {
    currency_ty: CurrencyId,
    source: Option<UserId>,
    quantity: i64,
    dest: Option<UserId>,
    happened_at: DateTime<Utc>,
    message_id: Option<i64>,
    to_motion: Option<i64>,
    to_votes: Option<i64>,
    comment: Option<String>,
    transfer_ty: Option<TransferType>,
    auction_id: Option<i64>,
}

#[must_use = "TransactionBuilder does nothing on its own, you must pass it to TransferHandler to cause an INSERT."]
impl TransactionBuilder {
    pub fn new(
        quantity: i64,
        currency: CurrencyId,
        happened_at: DateTime<Utc>,
    ) -> Self {
        Self{
            currency_ty: currency,
            source: None,
            quantity,
            dest: None,
            happened_at,
            message_id: None,
            to_motion: None,
            to_votes: None,
            comment: None,
            transfer_ty: None,
            auction_id: None,
        }
    }

    pub fn give(
        mut self,
        source: UserId,
        dest: UserId,
        admin: bool,
    ) -> Self {
        assert!(self.transfer_ty.is_none());
        self.source = Some(source);
        self.dest = Some(dest);
        self.transfer_ty = Some(if admin { TransferType::AdminGive } else { TransferType::Give });
        self
    }

    pub fn motion(
        mut self,
        source: UserId,
        motion_id: i64,
        num_votes: i64,
        create: bool,
    ) -> Self {
        assert!(self.transfer_ty.is_none());
        self.source = Some(source);
        self.to_motion = Some(motion_id);
        self.to_votes = Some(num_votes);
        self.transfer_ty = Some(if create { TransferType::MotionCreate } else { TransferType::MotionVote });
        self
    }

    pub fn fabricate(
        mut self,
        dest: UserId,
        generated: bool,
    ) -> Self {
        assert!(self.transfer_ty.is_none());
        self.dest = Some(dest);
        self.transfer_ty = Some(if generated { TransferType::Generated } else { TransferType::AdminFabricate });
        self
    }

    pub fn auction_create(
        mut self,
        source: UserId,
        auction_id: i64,
    ) -> Self {
        assert!(self.transfer_ty.is_none());
        self.auction_id = Some(auction_id);
        self.source = Some(source);
        self.transfer_ty = Some(TransferType::AuctionCreate);
        self
    }

    pub fn auction_reserve(
        mut self,
        source: UserId,
        auction: &AuctionWinner,
    ) -> Self {
        assert!(self.transfer_ty.is_none());
        assert!(self.currency_ty == auction.bid_ty);
        self.auction_id = Some(auction.auction_id);
        self.source = Some(source);
        self.transfer_ty = Some(TransferType::AuctionReserve);
        self
    }

    pub fn auction_refund(
        mut self,
        dest: UserId,
        auction: &AuctionWinner,
    ) -> Self {
        assert!(self.transfer_ty.is_none());
        assert!(self.currency_ty == auction.bid_ty);
        self.auction_id = Some(auction.auction_id);
        self.dest = Some(dest);
        self.transfer_ty = Some(TransferType::AuctionRefund);
        self
    }

    pub fn auction_payout(
        mut self,
        dest: UserId,
        auction: &AuctionWinner,
    ) -> Self {
        assert!(self.transfer_ty.is_none());
        assert!(self.currency_ty == auction.offer_ty);
        assert!(dest == auction.winner_id.unwrap());
        self.auction_id = Some(auction.auction_id);
        self.dest = Some(dest);
        self.transfer_ty = Some(TransferType::AuctionPayout);
        self
    }

    pub fn message_id(
        self,
        message_id: serenity::model::id::MessageId,
    ) -> Self {
        self.message_id_raw(message_id.0.try_into().unwrap())
    }

    pub fn message_id_raw(
        mut self,
        message_id: i64,
    ) -> Self {
        self.message_id = Some(message_id);
        self
    }

    pub fn comment(
        mut self,
        comment: String,
    ) -> Self {
        self.comment = Some(comment);
        self
    }
}

#[derive(Debug)]
pub enum TransferError {
    NotEnough,
    Overflow,
}

pub struct TransferHandler<'a> {
    conn: &'a diesel::pg::PgConnection,
    users_balances: HashMap<(UserId, CurrencyId), i64>,
}

impl<'a> TransferHandler<'a> {
    /// conn must already be in a transaction, else this will err
    pub fn new(
        conn: &'a diesel::pg::PgConnection,
        mut users: Vec<UserId>,
        mut currencies: Vec<CurrencyId>,
    ) -> diesel::result::QueryResult<Self> {
        users.sort();
        users.dedup();

        currencies.sort();
        currencies.dedup();

        let mut users_balances = HashMap::new();

        for u in users {
            for c in &currencies {
                let maybe_bal = bhdsl::balance_history
                    .select(bhdsl::balance)
                    .filter(bhdsl::user.eq(u))
                    .filter(bhdsl::ty.eq(&c))
                    .order((bhdsl::happened_at.desc(), bhdsl::rowid.desc(), bhdsl::sign.desc()))
                    .limit(1)
                    .for_update()
                    .get_result(conn)
                    .optional()?;
                
                if maybe_bal.is_none() {
                    diesel::dsl::sql_query("LOCK TABLE transfers").execute(conn)?;
                }
                let bal = maybe_bal.unwrap_or(0);
                users_balances.insert((u, c.clone()), bal);
            }
        }

        Ok(TransferHandler{conn, users_balances})
    }

    pub fn handle_single(
        conn: &'a diesel::pg::PgConnection,
        transfer: TransactionBuilder,
    ) -> Result<diesel::QueryResult<()>, TransferError> {
        let mut users = vec![];
        if let Some(user) = transfer.source { users.push(user); }
        if let Some(user) = transfer.dest { users.push(user); }
        let mut handle = match Self::new(
            conn,
            users,
            vec![transfer.currency_ty.clone()],
        ) {
            Ok(v) => v,
            Err(e) => return Ok(Err(e)),
        };
        handle.transfer(transfer)
    }

    pub fn balance(
        &self,
        user: UserId,
        currency: CurrencyId,
    ) -> i64 {
        self.users_balances[&(user, currency)]
    }

    pub fn transfer(
        &mut self,
        transfer: TransactionBuilder,
    ) -> Result<diesel::QueryResult<()>, TransferError> {
        self.transfer_impl(
            transfer.currency_ty,
            transfer.source,
            transfer.quantity,
            transfer.dest,
            transfer.message_id,
            transfer.to_motion,
            transfer.to_votes,
            transfer.comment,
            transfer.transfer_ty.unwrap(),
            transfer.auction_id,
            transfer.happened_at,
            false,
        )
    }

    fn transfer_impl(
        &mut self,
        ty: CurrencyId,
        maybe_from_user: Option<UserId>,
        quantity: i64,
        maybe_to_user: Option<UserId>,
        message_id: Option<i64>,
        to_motion: Option<i64>,
        to_votes: Option<i64>,
        comment: Option<String>,
        transfer_ty: crate::models::TransferType,
        auction_id: Option<i64>,
        happened_at: DateTime<Utc>,
        force: bool,
    ) -> Result<diesel::QueryResult<()>, TransferError> {
        let mut maybe_from_balance = None;
        if let Some(from_user) = maybe_from_user {
            let old_bal = self.users_balances[&(from_user, ty.clone())];
            if !force && old_bal < quantity {
                return Err(TransferError::NotEnough);
            }
            let new_balance = match old_bal.checked_sub(quantity) {
                Some(v) => v,
                None => return Err(TransferError::Overflow),
            };
            self.users_balances.insert((from_user, ty.clone()), new_balance).unwrap();
            maybe_from_balance = Some(new_balance);
        }
        let mut maybe_to_balance = None;
        if let Some(to_user) = maybe_to_user {
            let old_bal = self.users_balances[&(to_user, ty.clone())];
            let new_balance = match old_bal.checked_add(quantity) {
                Some(v) => v,
                None => return Err(TransferError::Overflow),
            };
            self.users_balances.insert((to_user, ty.clone()), new_balance);
            maybe_to_balance = Some(new_balance);
        }
        
        Ok(
            diesel::insert_into(tdsl::transfers)
                .values((
                    tdsl::ty.eq(ty),
                    tdsl::from_user.eq(maybe_from_user),
                    tdsl::from_balance.eq(maybe_from_balance),
                    tdsl::quantity.eq(quantity),
                    tdsl::to_user.eq(maybe_to_user),
                    tdsl::to_balance.eq(maybe_to_balance),
                    tdsl::message_id.eq(message_id),
                    tdsl::to_motion.eq(to_motion),
                    tdsl::to_votes.eq(to_votes),
                    tdsl::comment.eq(comment),
                    tdsl::transfer_ty.eq(transfer_ty),
                    tdsl::auction_id.eq(auction_id),
                    tdsl::happened_at.eq(happened_at),
                ))
                .execute(self.conn)
                .map(|_| ())
        )
    }
}