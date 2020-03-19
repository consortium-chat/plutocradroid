table! {
    gen (rowid) {
        rowid -> Int8,
        owner -> Nullable<Int8>,
        last_payout -> Timestamptz,
    }
}

table! {
    gen_transfers (rowid) {
        rowid -> Int8,
        from_user -> Nullable<Int8>,
        gen -> Int8,
        to_user -> Int8,
        happened_at -> Timestamptz,
        message_id -> Nullable<Int8>,
    }
}

table! {
    pc_transfers (rowid) {
        rowid -> Int8,
        from_user -> Nullable<Int8>,
        from_gen -> Nullable<Int8>,
        quantity -> Int8,
        to_user -> Int8,
        from_balance -> Nullable<Int8>,
        to_balance -> Int8,
        happened_at -> Timestamptz,
        message_id -> Nullable<Int8>,
    }
}

joinable!(gen_transfers -> gen (gen));
joinable!(pc_transfers -> gen (from_gen));

allow_tables_to_appear_in_same_query!(
    gen,
    gen_transfers,
    pc_transfers,
);
