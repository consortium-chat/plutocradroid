DROP FUNCTION fungible_transfer;

drop view balance_history;

create view balance_history as
  select "rowid", "from_user" as user, "from_balance" as balance, "quantity", -1 as sign, "happened_at", "ty"
  from transfers 
  where "from_user" is not null and "from_balance" is not null
  union all
  select "rowid", "to_user"   as user, "to_balance"   as balance, "quantity",  1 as sign, "happened_at", "ty"
  from transfers
;

ALTER TABLE transfers
  DROP COLUMN comment;