table! {
    use diesel::sql_types::*;
    use crate::models::Transfer_type;

    balance_history (rowid, sign) {
        rowid -> Int8,
        user -> Int8,
        balance -> Int8,
        quantity -> Int8,
        sign -> Int4,
        happened_at -> Timestamptz,
        ty -> Text,
        comment -> Nullable<Text>,
        other_party -> Nullable<Int8>,
        message_id -> Nullable<Int8>,
        to_motion -> Nullable<Int8>,
        to_votes -> Nullable<Int8>,
        transfer_ty -> Transfer_type,
        auction_id -> Nullable<Int8>,
    }
}

table! {
    auction_and_winner (auction_id) {
        auction_id -> Int8,
        created_at -> Timestamptz,
        auctioneer -> Nullable<Int8>,
        offer_ty -> Text,
        offer_amt -> Int8,
        bid_ty -> Text,
        bid_min -> Int8,
        finished -> Bool,
        max_bid_user -> Nullable<Int8>,
        max_bid_amt -> Nullable<Int8>,

        last_change -> Timestamptz,
        transfer_id -> Nullable<Int8>,
        winner_id -> Nullable<Int8>,
        winner_bid -> Nullable<Int8>,
        bid_at -> Nullable<Timestamptz>,
        last_timer_bump -> Timestamptz,
    }
}