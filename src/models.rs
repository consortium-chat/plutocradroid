use std::borrow::Cow;
use chrono::{DateTime,Utc};
use diesel_derive_enum::DbEnum;

use crate::schema::{motions, motion_votes as mv};
use crate::view_schema::auction_and_winner as anw;

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

    pub fn cols() -> (
        motions::rowid,
        motions::bot_message_id,
        motions::motion_text,
        motions::motioned_at,
        motions::last_result_change,
        motions::is_super,
        motions::announcement_message_id,
    ) {
        (
            motions::rowid,
            motions::bot_message_id,
            motions::motion_text,
            motions::motioned_at,
            motions::last_result_change,
            motions::is_super,
            motions::announcement_message_id,
        )
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

#[derive(Copy,Clone,Debug,Serialize,Queryable)]
pub struct MotionVote {
    pub user:i64,
    pub direction:bool,
    pub amount:i64,
}

impl MotionVote {
    pub fn cols() -> (
        mv::user,
        mv::direction,
        mv::amount,
    ) {
        (
            mv::user,
            mv::direction,
            mv::amount,
        )
    }
}


#[derive(Debug, PartialEq, Eq, Clone, Queryable)]
pub struct ItemType{
    pub name: String,
    pub long_name_plural: String,
    pub long_name_ambiguous: String,
}

use crate::schema::item_types;

impl ItemType {
    pub fn db_name(&self) -> &str {
        self.name.as_str()
    }

    pub fn cols() -> (
        item_types::name,
        item_types::long_name_plural,
        item_types::long_name_ambiguous,
    ) {
        item_types::all_columns
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
    pub offer_ty: String,
    pub offer_amt: i32,
    pub bid_ty: String,
    pub bid_min: i32,
    pub finished: bool,
    pub last_change: DateTime<Utc>,
    pub winner_id: Option<i64>,
    pub winner_bid: Option<i64>,
}

impl AuctionWinner {
    pub fn current_min_bid(&self) -> i32 { self.winner_bid.map(|n| (n as i32) + 1).unwrap_or(self.bid_min) }
    pub fn end_at(&self) -> DateTime<Utc> { self.last_change + *crate::AUCTION_EXPIRATION }
    pub fn damm(&self) -> String { crate::damm::add_to_str(self.auction_id.to_string()) }
    pub fn cols() -> (
        anw::auction_id,
        anw::created_at,
        anw::auctioneer,
        anw::offer_ty,
        anw::offer_amt,
        anw::bid_ty,
        anw::bid_min,
        anw::finished,
        anw::last_change,
        anw::winner_id,
        anw::winner_bid,
    ) {
        (
            anw::auction_id,
            anw::created_at,
            anw::auctioneer,
            anw::offer_ty,
            anw::offer_amt,
            anw::bid_ty,
            anw::bid_min,
            anw::finished,
            anw::last_change,
            anw::winner_id,
            anw::winner_bid,
        )
    }
}