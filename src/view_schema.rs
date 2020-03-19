table! {
    balance_history (rowid, sign) {
        rowid -> Int8,
        user -> Int8,
        balance -> Int8,
        quantity -> Int8,
        sign -> Int4,
        happened_at -> Timestamptz,
        ty -> Text,
    }
}