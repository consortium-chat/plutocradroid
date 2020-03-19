table! {
    item_types (name) {
        name -> Text,
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
        to_user -> Int8,
        from_balance -> Nullable<Int8>,
        to_balance -> Int8,
        happened_at -> Timestamptz,
        message_id -> Nullable<Int8>,
    }
}

joinable!(transfers -> item_types (ty));

allow_tables_to_appear_in_same_query!(
    item_types,
    single,
    transfers,
);
