-- Your SQL goes here
CREATE OR REPLACE FUNCTION fungible_fabricate(
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
        select balance into   to_balance from balance_history where balance_history."user" =   touser and balance_history.ty = fungible_type order by happened_at desc limit 1 for update;
        IF to_balance IS NULL THEN
            to_balance := 0;
        END IF;
        to_balance := to_balance + amount;
        INSERT INTO transfers ("from_user", "quantity", "to_user", "from_balance", "to_balance", "happened_at", "message_id", "ty", "comment")
                       VALUES ( NULL      ,  amount   ,  touser  ,  NULL         ,  to_balance ,  NOW()       ,  NULL       ,  fungible_type, comment);
        RETURN 'done';
    END;
    $$
    LANGUAGE plpgsql;
