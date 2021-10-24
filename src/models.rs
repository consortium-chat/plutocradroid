use std::borrow::Cow;
use chrono::{DateTime,Utc};
use diesel_derive_enum::DbEnum;

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
}