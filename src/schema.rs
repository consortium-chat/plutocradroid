table! {
    use diesel::sql_types::*;
    use crate::models::Transfer_type;

    auctions (rowid) {
        rowid -> Int8,
        created_at -> Timestamptz,
        auctioneer -> Nullable<Int8>,
        offer_ty -> Text,
        offer_amt -> Int4,
        bid_ty -> Text,
        bid_min -> Int4,
        finished -> Bool,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::models::Transfer_type;

    item_type_aliases (alias) {
        name -> Text,
        alias -> Text,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::models::Transfer_type;

    item_types (name) {
        name -> Text,
        long_name_plural -> Text,
        long_name_ambiguous -> Text,
        position -> Int4,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::models::Transfer_type;

    motion_votes (user, motion) {
        user -> Int8,
        motion -> Int8,
        direction -> Bool,
        amount -> Int8,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::models::Transfer_type;

    motions (rowid) {
        rowid -> Int8,
        command_message_id -> Int8,
        bot_message_id -> Int8,
        motion_text -> Text,
        motioned_at -> Timestamptz,
        last_result_change -> Timestamptz,
        is_super -> Bool,
        announcement_message_id -> Nullable<Int8>,
        needs_update -> Bool,
        motioned_by -> Int8,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::models::Transfer_type;

    single (enforce_single_row) {
        enforce_single_row -> Bool,
        last_gen -> Timestamptz,
        last_task_run -> Timestamptz,
        last_auto_auction -> Nullable<Timestamptz>,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::models::Transfer_type;

    thing_ids (rowid) {
        rowid -> Int8,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::models::Transfer_type;

    transfers (rowid) {
        rowid -> Int8,
        ty -> Text,
        from_user -> Nullable<Int8>,
        quantity -> Int8,
        to_user -> Nullable<Int8>,
        from_balance -> Nullable<Int8>,
        to_balance -> Nullable<Int8>,
        happened_at -> Timestamptz,
        message_id -> Nullable<Int8>,
        to_motion -> Nullable<Int8>,
        to_votes -> Nullable<Int8>,
        comment -> Nullable<Text>,
        transfer_ty -> Transfer_type,
        auction_id -> Nullable<Int8>,
    }
}

joinable!(auctions -> thing_ids (rowid));
joinable!(item_type_aliases -> item_types (name));
joinable!(motion_votes -> motions (motion));
joinable!(motions -> thing_ids (rowid));
joinable!(transfers -> auctions (auction_id));
joinable!(transfers -> item_types (ty));

allow_tables_to_appear_in_same_query!(
    auctions,
    item_type_aliases,
    item_types,
    motion_votes,
    motions,
    single,
    thing_ids,
    transfers,
);
