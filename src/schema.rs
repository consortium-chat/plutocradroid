table! {
    item_types (name) {
        name -> Text,
    }
}

table! {
    motion_ids (rowid) {
        rowid -> Int8,
    }
}

table! {
    motions (rowid) {
        rowid -> Int8,
        command_message_id -> Int8,
        bot_message_id -> Int8,
        motion_text -> Text,
        motioned_at -> Timestamptz,
        last_result_change -> Timestamptz,
        is_super -> Bool,
        announcement_message_id -> Nullable<Int8>,
    }
}

table! {
    motion_votes (user, motion) {
        user -> Int8,
        motion -> Int8,
        direction -> Bool,
        amount -> Int8,
    }
}

table! {
    single (enforce_single_row) {
        enforce_single_row -> Bool,
        last_gen -> Timestamptz,
    }
}

table! {
    transfers (rowid) {
        rowid -> Int8,
        ty -> Text,
        from_user -> Nullable<Int8>,
        from_gen -> Bool,
        quantity -> Int8,
        to_user -> Nullable<Int8>,
        from_balance -> Nullable<Int8>,
        to_balance -> Nullable<Int8>,
        happened_at -> Timestamptz,
        message_id -> Nullable<Int8>,
        to_motion -> Nullable<Int8>,
        to_votes -> Nullable<Int8>,
    }
}

joinable!(motion_votes -> motions (motion));
joinable!(motions -> motion_ids (rowid));
joinable!(transfers -> item_types (ty));

allow_tables_to_appear_in_same_query!(
    item_types,
    motion_ids,
    motions,
    motion_votes,
    single,
    transfers,
);
