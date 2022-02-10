use std::borrow::Cow;
use std::convert::{TryFrom,TryInto};
use chrono::{DateTime,Utc};
use diesel::backend::Backend;
use diesel::deserialize::Queryable;
use diesel::sql_types::Int8;
use diesel::deserialize;
use diesel::serialize;
use diesel_derive_enum::DbEnum;
use crate::transfers::CurrencyId;

// Thanks to Chayum Friedman https://stackoverflow.com/a/69877842/1267729
macro_rules! impl_cols {
    (@build-tuple
        ( $($types:path,)* )
        ( $($ns:ident)::* )
        ( $col_name:ident, $($rest:tt)* )
    ) => {
        impl_cols! { @build-tuple
            (
                $($types,)* 
                $($ns::)* $col_name,
            )
            ( $($ns)::* )
            ( $($rest)* )
        }
    };
    // Empty case
    (@build-tuple
        ( $($types:path,)* )
        ( $($ns:ident)::* )
        ( )
    ) => {
        ( $($types,)* )
    };
    (
        $($ns:ident)::*,
        $($col_name:ident,)*
    ) => {
        pub fn cols() -> impl_cols! { @build-tuple
            ( )
            ( $($ns)::* )
            ( $($col_name,)* )
        } {
            impl_cols! { @build-tuple
                ( )
                ( $($ns)::* )
                ( $($col_name,)* )
            }
        }
    };
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Hash,FromSqlRow,AsExpression)]
#[sql_type = "Int8"]
pub struct UserId(u64);

impl UserId {
    pub fn into_i64(self) -> i64 {
        self.0.try_into().unwrap()
    }

    pub fn into_u64(self) -> u64 {
        self.0
    }

    pub fn into_serenity(self) -> serenity::model::id::UserId {
        serenity::model::id::UserId(self.0)
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        <u64 as std::fmt::Display>::fmt(&self.0, f)
    }
}

impl From<UserId> for serenity::model::id::UserId {
    fn from(v: UserId) -> serenity::model::id::UserId {
        v.into_serenity()
    }
}

impl From<serenity::model::id::UserId> for UserId {
    fn from(v: serenity::model::id::UserId) -> Self {
        Self(v.0)
    }
}

impl TryFrom<i64> for UserId {
    type Error = ();

    fn try_from(from: i64) -> Result<Self, Self::Error> {
        match from {
            v @ 0.. => Ok(UserId(v as u64)),
            _ => Err(()),
        }
    }
}

impl TryFrom<u64> for UserId {
    type Error = ();

    fn try_from(from: u64) -> Result<Self, Self::Error> {
        match from {
            v @ 0..=9223372036854775807 => Ok(UserId(v)),
            _ => Err(()),
        }
    }
}

impl deserialize::FromSql<Int8, diesel::pg::Pg> for UserId
where
    i64: deserialize::FromSql<Int8, diesel::pg::Pg>,
{
    fn from_sql(bytes: Option<&<diesel::pg::Pg as Backend>::RawValue>) -> deserialize::Result<Self> {
        match <i64 as deserialize::FromSql<Int8, diesel::pg::Pg>>::from_sql(bytes)? {
            v @ 0.. => Ok(UserId(u64::try_from(v).unwrap())),
            v => Err(format!("Invalid user id {}", v).into()),
        }
    }
}

impl serialize::ToSql<Int8, diesel::pg::Pg> for UserId
where
    i64: serialize::ToSql<Int8, diesel::pg::Pg>,
{
    fn to_sql<W: std::io::Write>(
        &self,
        out: &mut serialize::Output<W, diesel::pg::Pg>
    ) -> serialize::Result {
        <i64 as serialize::ToSql<Int8, diesel::pg::Pg>>::to_sql(&self.into_i64(), out)
    }
}

#[derive(Clone,Debug,Serialize,Queryable)]
pub struct Motion<'a> {
    pub rowid:i64,
    pub bot_message_id:i64,
    pub motion_text:Cow<'a, str>,
    pub motioned_at:DateTime<Utc>,
    pub last_result_change:DateTime<Utc>,
    pub is_super:bool,
    pub announcement_message_id:Option<i64>,
}

#[derive(Clone,Debug,Serialize)]
pub struct MotionWithCount<'a> {
    pub rowid:i64,
    pub bot_message_id:i64,
    pub motion_text:Cow<'a, str>,
    pub motioned_at:DateTime<Utc>,
    pub last_result_change:DateTime<Utc>,
    pub is_super:bool,
    pub announcement_message_id:Option<i64>,
    pub yes_vote_count:u64,
    pub no_vote_count:u64,
    pub is_win:bool,
}

impl<'a> Motion<'a> {
    #[allow(dead_code)]
    pub fn damm_id(&self) -> String {
        crate::damm::add_to_str(format!("{}",self.rowid))
    }

    impl_cols!{
        crate::schema::motions::dsl,
        rowid,
        bot_message_id,
        motion_text,
        motioned_at,
        last_result_change,
        is_super,
        announcement_message_id,
    }
}

impl<'a> MotionWithCount<'a>{
    pub fn from_motion(m: Motion, yes_vote_count: u64, no_vote_count: u64) -> MotionWithCount {
        MotionWithCount{
            rowid: m.rowid,
            bot_message_id: m.bot_message_id,
            motion_text: m.motion_text,
            motioned_at: m.motioned_at,
            last_result_change: m.last_result_change,
            is_super: m.is_super,
            announcement_message_id: m.announcement_message_id,
            yes_vote_count,
            no_vote_count,
            is_win: crate::is_win::is_win(yes_vote_count as i64, no_vote_count as i64, m.is_super),
        }
    }

    pub fn damm_id(&self) -> String {
        crate::damm::add_to_str(format!("{}",self.rowid))
    }

    pub fn end_at(&self) -> DateTime<Utc> {
        self.last_result_change + *crate::MOTION_EXPIRATION
    }
}

#[derive(Copy,Clone,Debug,Queryable)]
pub struct MotionVote {
    pub user:UserId,
    pub direction:bool,
    pub amount:i64,
}

impl MotionVote {
    impl_cols!{
        crate::schema::motion_votes::dsl,
        user,
        direction,
        amount,
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Queryable)]
pub struct ItemType {
    pub id: CurrencyId,
    pub long_name_plural: String,
    pub long_name_ambiguous: String,
}

impl ItemType {
    pub fn db_name(&self) -> &str {
        self.id.as_str()
    }

    impl_cols!{
        crate::schema::item_types,
        name,
        long_name_plural,
        long_name_ambiguous,
    }
}

// create type transfer_type as enum (
//     'motion_create',
//     'motion_vote',
//     'generated',
//     'admin_fabricate',
//     'admin_give',
//     'give',
//     'command_fabricate',
//     --new
//     'auction_create', --you've offered up some fungibles for bid
//     'auction_reserve', --placing a bid, fungibles are held
//     'auction_refund' --someone else outbid you, held fungibles are returned
// );
#[derive(Copy,Clone,PartialEq,Eq,Debug,DbEnum)]
#[DieselType = "Transfer_type"]
pub enum TransferType {
    MotionCreate,
    MotionVote,
    Generated,
    AdminFabricate,
    AdminGive,
    Give,
    CommandFabricate,
    AuctionCreate,
    AuctionReserve,
    AuctionRefund,
    AuctionPayout,
}


#[derive(Debug,Clone,PartialEq,Eq,Queryable)]
pub struct AuctionWinner {
    pub auction_id: i64,
    pub created_at: DateTime<Utc>,
    pub auctioneer: Option<i64>,
    pub offer_ty: CurrencyId,
    pub offer_amt: i64,
    pub bid_ty: CurrencyId,
    pub bid_min: i64,
    pub finished: bool,
    pub last_change: DateTime<Utc>,
    pub winner_id: Option<UserId>,
    pub winner_bid: Option<i64>,
    pub winner_bid_at: Option<DateTime<Utc>>,
    pub last_timer_bump: DateTime<Utc>,
    pub max_bid_user: Option<UserId>,
    pub max_bid_amt: Option<i64>,
}

pub struct MaxBid {
    pub user: UserId,
    pub currency: CurrencyId,
    pub amount: i64,
}

impl AuctionWinner {
    pub fn current_min_bid(&self) -> i64 { self.winner_bid.map(|n| n.checked_add(1).unwrap()).unwrap_or(self.bid_min) }
    pub fn end_at(&self) -> DateTime<Utc> { self.last_timer_bump + *crate::AUCTION_EXPIRATION }
    pub fn damm(&self) -> String { crate::damm::add_to_str(self.auction_id.to_string()) }
    pub fn auctioneer_name(&self) -> Cow<'static, str> {
        self.auctioneer.map(|a| crate::names::name_of(serenity::model::id::UserId::from(a as u64))).unwrap_or_else(|| "The CONsortium".into())
    }
    pub fn winner(&self) -> Option<(UserId, i64)> {
        self.winner_id.map(|winner| (winner, self.winner_bid.unwrap()))
    }
    pub fn max_bid(&self) -> Option<MaxBid> {
        self.max_bid_amt.map(|amount| {
            MaxBid{
                user: self.max_bid_user.unwrap(),
                currency: self.bid_ty.clone(),
                amount
            }
        })
    }
    impl_cols!{
        crate::view_schema::auction_and_winner,
        auction_id,
        created_at,
        auctioneer,
        offer_ty,
        offer_amt,
        bid_ty,
        bid_min,
        finished,
        last_change,
        winner_id,
        winner_bid,
        bid_at,
        last_timer_bump,
        max_bid_user,
        max_bid_amt,
    }
}

//     Column    |           Type           | Collation | Nullable |                 Default
// --------------+--------------------------+-----------+----------+------------------------------------------
//  rowid        | bigint                   |           | not null | nextval('transfers_rowid_seq'::regclass)
//  ty           | text                     |           | not null |
//  from_user    | bigint                   |           |          |
//  quantity     | bigint                   |           | not null |
//  to_user      | bigint                   |           |          |
//  from_balance | bigint                   |           |          |
//  to_balance   | bigint                   |           |          |
//  happened_at  | timestamp with time zone |           | not null |
//  message_id   | bigint                   |           |          |
//  to_motion    | bigint                   |           |          |
//  to_votes     | bigint                   |           |          |
//  comment      | text                     |           |          |
//  transfer_ty  | transfer_type            |           | not null |
//  auction_id   | bigint                   |           |          |

#[derive(Debug,Clone,PartialEq,Eq,Queryable)]
pub struct RawTransfer {
    pub rowid: i64,
    pub ty: CurrencyId,
    pub from_user: Option<UserId>,
    pub quantity: i64,
    pub to_user: Option<UserId>,
    pub from_balance: Option<i64>,
    pub to_balance: Option<i64>,
    pub happened_at: DateTime<Utc>,
    pub message_id: Option<i64>,
    pub to_motion: Option<i64>,
    pub to_votes: Option<i64>,
    pub comment: Option<String>,
    pub transfer_ty: TransferType,
    pub auction_id: Option<i64>,
}

impl RawTransfer {
    impl_cols! {
        crate::schema::transfers::dsl,
        rowid,
        ty,
        from_user,
        quantity,
        to_user,
        from_balance,
        to_balance,
        happened_at,
        message_id,
        to_motion,
        to_votes,
        comment,
        transfer_ty,
        auction_id,
    }

    fn from(&self) -> Option<UserBal> {
        self.from_user.map(|u| UserBal{ty: self.ty.clone(), user: u, bal: self.from_balance.unwrap()})
    }

    fn to(&self) -> Option<UserBal> {
        self.to_user.map(|u| UserBal{ty: self.ty.clone(), user: u, bal: self.to_balance.unwrap()})
    }
}

#[allow(dead_code, unused_variables)]
fn __test_transfer_types() {
    if false { //build only
        use diesel::prelude::*;
        let conn = diesel::PgConnection::establish("abc").unwrap();
        let data:Vec<RawTransfer> = crate::schema::transfers::dsl::transfers.select(RawTransfer::cols()).get_results(&conn).unwrap();
        let data:Vec<Transfer> = crate::schema::transfers::dsl::transfers.select(Transfer::cols()).get_results(&conn).unwrap();
    }
}

#[derive(Debug,Clone,PartialEq,Eq)]
pub struct UserBal {
    pub user: UserId,
    pub ty: CurrencyId,
    pub bal: i64,
}

impl UserBal {
    pub fn discord_id(&self) -> serenity::model::id::UserId {
        self.user.into_serenity()
    }
}

#[derive(Debug,Clone,PartialEq,Eq)]
pub enum TransferExtra {
    Motion{from:UserBal, motion_id:i64, votes: i64, created:bool},
    ThinAir{to:UserBal, generated:bool},
    Give{to:UserBal, from:UserBal, admin: bool},
    AuctionCreate{ auction_id:i64, from:UserBal},
    AuctionReserve{auction_id:i64, from:UserBal},
    AuctionRefund{ auction_id:i64, to:UserBal},
    AuctionPayout{ auction_id:i64, to:UserBal},
}

#[derive(Debug,Clone,PartialEq,Eq)]
pub struct Transfer {
    pub rowid: i64,
    pub ty: CurrencyId,
    pub happened_at: DateTime<Utc>,
    pub message_id: Option<i64>,
    pub quantity: i64,
    pub comment: Option<String>,
    pub extra: TransferExtra,
}

impl Transfer {
    impl_cols! {
        crate::schema::transfers::dsl,
        rowid,
        ty,
        from_user,
        quantity,
        to_user,
        from_balance,
        to_balance,
        happened_at,
        message_id,
        to_motion,
        to_votes,
        comment,
        transfer_ty,
        auction_id,
    }
}

impl<DB: diesel::backend::Backend, ST> Queryable<ST, DB> for Transfer
where
    RawTransfer: diesel::deserialize::Queryable<ST, DB>,
{
    type Row = <RawTransfer as Queryable<ST,DB>>::Row;

    fn build(row: Self::Row) -> Self {
        <RawTransfer as Queryable<ST,DB>>::build(row).into()
    }
}

impl From<RawTransfer> for Transfer {
    fn from(r: RawTransfer) -> Self {
        let rowid = r.rowid;
        let ty = r.ty.clone();
        let happened_at = r.happened_at;
        let message_id = r.message_id;
        let comment = r.comment.clone();
        let quantity = r.quantity;
        let extra = match r.transfer_ty {
            TransferType::MotionCreate | TransferType::MotionVote => TransferExtra::Motion{
                from: r.from().unwrap(),
                motion_id: r.to_motion.unwrap(),
                votes: r.to_votes.unwrap(),
                created: matches!(r.transfer_ty, TransferType::MotionCreate)
            },
            TransferType::Generated | TransferType::AdminFabricate | TransferType::CommandFabricate => TransferExtra::ThinAir{
                to: r.to().unwrap(),
                generated: matches!(r.transfer_ty, TransferType::Generated),
            },
            TransferType::Give | TransferType::AdminGive => TransferExtra::Give{
                from: r.from().unwrap(),
                to: r.to().unwrap(),
                admin: matches!(r.transfer_ty, TransferType::AdminGive),
            },
            TransferType::AuctionCreate => TransferExtra::AuctionCreate{
                auction_id: r.auction_id.unwrap(),
                from: r.from().unwrap(),
            },
            TransferType::AuctionReserve => TransferExtra::AuctionReserve{
                auction_id: r.auction_id.unwrap(),
                from: r.from().unwrap(),
            },
            TransferType::AuctionRefund => TransferExtra::AuctionRefund{
                auction_id: r.auction_id.unwrap(),
                to: r.to().unwrap(),
            },
            TransferType::AuctionPayout => TransferExtra::AuctionPayout{
                auction_id: r.auction_id.unwrap(),
                to: r.to().unwrap(),
            },
        };

        Transfer{
            rowid,
            ty,
            happened_at,
            message_id,
            quantity,
            comment,
            extra,
        }
    }
}