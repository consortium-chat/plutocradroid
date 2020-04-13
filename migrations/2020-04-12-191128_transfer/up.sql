ALTER TABLE transfers
  ADD COLUMN comment text;

create or replace view balance_history as
  select "rowid", "from_user" as user, "from_balance" as balance, "quantity", -1 as sign, "happened_at", "ty", "comment"
  from transfers 
  where "from_user" is not null and "from_balance" is not null
  union all
  select "rowid", "to_user"   as user, "to_balance"   as balance, "quantity",  1 as sign, "happened_at", "ty", "comment"
  from transfers
;

CREATE OR REPLACE FUNCTION fungible_transfer(
        fromuser bigint,
        touser bigint,
        amount bigint,
        fungible_type text,
        comment text
    ) returns text
    AS $$
    DECLARE
        from_balance bigint;
        to_balance bigint;
    BEGIN
        IF amount < 1 THEN
            RETURN 'invalid amount';
        END IF;
        select balance into from_balance from balance_history where balance_history."user" = fromuser and balance_history.ty = fungible_type order by happened_at desc limit 1 for update;
        select balance into   to_balance from balance_history where balance_history."user" =   touser and balance_history.ty = fungible_type order by happened_at desc limit 1 for update;
        IF from_balance IS NULL THEN
            from_balance := 0;
        END IF;
        IF to_balance IS NULL THEN
            to_balance := 0;
        END IF;
        IF from_balance < amount THEN
            RETURN 'not enough fungibles';
        END IF;
        from_balance := from_balance - amount;
        to_balance := to_balance + amount;
        INSERT INTO transfers ("from_user", "quantity", "to_user", "from_balance", "to_balance", "happened_at", "message_id", "ty", "comment")
                       VALUES ( fromuser  ,  amount   ,  touser  ,  from_balance ,  to_balance ,  NOW()       ,  NULL       ,  fungible_type, comment);
        RETURN 'done';
    END;
    $$
    LANGUAGE plpgsql;