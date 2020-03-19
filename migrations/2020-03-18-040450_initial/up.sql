create table gen (
    "rowid" serial8 primary key,
    "owner" int8,
    "last_payout" timestamptz not null --this is not when the last payout *did* happen, but when the last payout *should* have happened.
);

create index gen_owner on gen("owner");
create index gen_last_payout on gen("last_payout");

create table gen_transfers (
    "rowid" serial8 primary key, --diesel only supports tables with primary keys
    "from_user" int8,
    "gen" int8 not null references gen(rowid),
    "to_user" int8 not null,
    "happened_at" timestamptz not null,
    "message_id" int8
);

create index gen_transfers_from_user_happened_at ON gen_transfers("from_user", "happened_at");
create index gen_transfers_to_user_happened_at ON gen_transfers("to_user", "happened_at");

create table pc_transfers (
    "rowid" serial8 primary key, --diesel only supports tables with primary keys
    "from_user" int8,
    "from_gen" int8 references gen(rowid),
    "quantity" int8 not null,
    "to_user" int8 not null,

    "from_balance" int8, --if from_user is not null, from_user's balance after this transaction
    "to_balance" int8 not null, --to_user's balance after this transaction

    "happened_at" timestamptz not null,
    "message_id" int8
);

create index pc_transfers_from_user_happened_at ON pc_transfers("from_user", "happened_at");
create index pc_transfers_to_user_happened_at ON pc_transfers("to_user", "happened_at");

create view balance_history as
  select "rowid", "from_user" as user, "from_balance" as balance, "quantity", -1 as sign, "happened_at"
  from pc_transfers 
  where "from_user" is not null and "from_balance" is not null
  union all
  select "rowid", "to_user"   as user, "to_balance"   as balance, "quantity",  1 as sign, "happened_at"
  from pc_transfers
;