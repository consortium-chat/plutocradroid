table! {
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
        transfer_ty -> Text,
    }
}