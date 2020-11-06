create or replace view balance_history as
  select
    "rowid",
    "from_user" as user,
    "from_balance" as balance,
    "quantity",
    -1 as sign,
    "happened_at",
    "ty",
    "comment",
    "to_user" as other_party,
    "message_id",
    "to_motion",
    "to_votes",
    false as "from_gen"
  from transfers 
  where
    "from_user" is not null
  and
    "from_balance" is not null
  union all
  select
    "rowid",
    "to_user" as user,
    "to_balance" as balance,
    "quantity",
    1 as sign,
    "happened_at",
    "ty",
    "comment",
    "from_user" as other_party,
    "message_id",
    NULL::bigint as "to_motion",
    NULL::bigint as "to_votes",
    "from_gen"
  from transfers
;

-- CHECK from_user IS NULL = from_balance IS NULL
-- CHECK to_motion IS NULL = to_votes IS NULL