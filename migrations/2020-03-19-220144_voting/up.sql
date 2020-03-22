alter table transfers alter to_user drop not null;
alter table transfers alter to_balance drop not null;
alter table transfers add column to_motion int8;
alter table transfers add column to_votes int8;

create or replace view balance_history as
  select "rowid", "from_user" as user, "from_balance" as balance, "quantity", -1 as sign, "happened_at", "ty"
  from transfers 
  where "from_user" is not null and "from_balance" is not null
  union all
  select "rowid", "to_user"   as user, "to_balance"   as balance, "quantity",  1 as sign, "happened_at", "ty"
  from transfers
  where "to_user" is not null and "to_balance" is not null
;

create table motion_ids (
  rowid serial8 primary key
);

create table motions (
    rowid int8 primary key references motion_ids(rowid),
    command_message_id int8 not null,
    bot_message_id int8 not null,
    motion_text text not null,
    motioned_at timestamptz not null,
    last_result_change timestamptz not null,
    is_super boolean not null,
    announcement_message_id int8
);

create table motion_votes (
    "user" int8 not null,
    "motion" int8 not null references motions("rowid"),
    "direction" boolean not null, --true: in favor; false: against
    "amount" int8 not null,
    primary key ("user", "motion")
);