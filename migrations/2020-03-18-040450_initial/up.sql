create table item_types (
    "name" text not null primary key
);

insert into item_types values ('pc'),('gen');

create table transfers (
    "rowid" serial8 primary key, --diesel only supports tables with primary keys
    "ty" text not null references item_types("name"),
    "from_user" int8,
    "from_gen" boolean not null default false,
    "quantity" int8 not null,
    "to_user" int8 not null,

    "from_balance" int8, --if from_user is not null, from_user's balance after this transaction
    "to_balance" int8 not null, --to_user's balance after this transaction

    "happened_at" timestamptz not null,
    "message_id" int8
);

create index transfers_from_user_happened_at ON transfers("from_user", "happened_at");
create index transfers_to_user_happened_at ON transfers("to_user", "happened_at");
create index transfers_ty_from_user_happened_at ON transfers("ty", "from_user", "happened_at");
create index transfers_ty_to_user_happened_at ON transfers("ty", "to_user", "happened_at");


create view balance_history as
  select "rowid", "from_user" as user, "from_balance" as balance, "quantity", -1 as sign, "happened_at", "ty"
  from transfers 
  where "from_user" is not null and "from_balance" is not null
  union all
  select "rowid", "to_user"   as user, "to_balance"   as balance, "quantity",  1 as sign, "happened_at", "ty"
  from transfers
;

create table single (
    enforce_single_row boolean primary key CHECK(enforce_single_row),
    last_gen timestamptz not null
);