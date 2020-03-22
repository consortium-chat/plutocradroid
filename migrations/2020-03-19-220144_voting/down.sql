create or replace view balance_history as
  select "rowid", "from_user" as user, "from_balance" as balance, "quantity", -1 as sign, "happened_at", "ty"
  from transfers 
  where "from_user" is not null and "from_balance" is not null
  union all
  select "rowid", "to_user"   as user, "to_balance"   as balance, "quantity",  1 as sign, "happened_at", "ty"
  from transfers
;

alter table transfers alter to_user set not null;
alter table transfers alter to_balance set not null;
alter table transfers drop column to_motion;
alter table transfers drop column to_votes;

drop table motion_votes;

drop table motions;

drop table motion_ids;
