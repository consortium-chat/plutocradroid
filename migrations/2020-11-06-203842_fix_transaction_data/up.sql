drop view balance_history;
alter table transfers add constraint is_motion check ((to_motion IS NOT NULL) = (to_votes IS NOT NULL));
alter table transfers add constraint makes_sense check (to_user IS NOT NULL or from_user IS NOT NULL);
alter table transfers add column transfer_ty text; --not null
alter table motions add column motioned_by bigint; --not null

update motions m set motioned_by = t.from_user from transfers t where m.command_message_id = t.message_id;
update transfers t set transfer_ty = 'motion_create', to_motion = m.rowid, to_votes = 1 from motions m where m.command_message_id = t.message_id;

alter table motions alter column motioned_by set not null;

update transfers t set transfer_ty = 'motion_vote' where t.to_motion IS NOT NULL and t.transfer_ty IS NULL;
update transfers t set transfer_ty = 'generated' where t.from_gen and t.transfer_ty IS NULL;

-- (has comment -> made from sql command) may not hold up in the future, but should be good for now
update transfers t set transfer_ty = 'admin_fabricate' where t.comment IS NOT NULL and t.from_user IS NULL and t.transfer_ty IS NULL;
update transfers t set transfer_ty = 'admin_give' where t.comment IS NOT NULL and t.from_user IS NOT NULL and t.transfer_ty IS NULL;

update transfers t set transfer_ty = 'give' where t.to_user IS NOT NULL and t.from_user IS NOT NULL and t.transfer_ty IS NULL;
update transfers t set transfer_ty = 'command_fabricate' where t.message_id IS NOT NULL and t.from_user IS NULL and t.transfer_ty IS NULL;

alter table transfers alter column transfer_ty set not null;
alter table transfers drop column from_gen;
alter table transfers add constraint transfer_ty_enum check (transfer_ty IN ('motion_create', 'motion_vote', 'generated', 'admin_fabricate', 'admin_give', 'give', 'command_fabricate'));
alter table transfers add constraint motion_matches_ty check ((to_motion IS NOT NULL) = transfer_ty IN ('motion_create', 'motion_vote'));
alter table transfers add constraint give_has_both_sides check ((NOT (transfer_ty IN ('give', 'admin_give'))) OR (from_user IS NOT NULL and to_user IS NOT NULL));
alter table transfers add constraint from_eq_from check ((from_user IS NOT NULL) = (from_balance IS NOT NULL));
alter table transfers add constraint to_eq_to check ((to_user IS NOT NULL) = (to_balance IS NOT NULL));

create index on transfers (from_user, transfer_ty, happened_at);
create index on transfers (to_user, transfer_ty, happened_at);
create index on transfers (ty, from_user, transfer_ty, happened_at);
create index on transfers (ty, to_user, transfer_ty, happened_at);

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
    "transfer_ty"
  from transfers 
  where
    "from_user" is not null
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
    "transfer_ty"
  from transfers
  where
    "to_user" is not null
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
        INSERT INTO transfers ("from_user", "quantity", "to_user", "from_balance", "to_balance", "happened_at", "message_id", "ty", "comment", "transfer_ty")
                       VALUES ( fromuser  ,  amount   ,  touser  ,  from_balance ,  to_balance ,  NOW()       ,  NULL       ,  fungible_type, comment, 'admin_give');
        RETURN 'done';
    END;
    $$
    LANGUAGE plpgsql;

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
        INSERT INTO transfers ("from_user", "quantity", "to_user", "from_balance", "to_balance", "happened_at", "message_id", "ty", "comment", "transfer_ty")
                       VALUES ( NULL      ,  amount   ,  touser  ,  NULL         ,  to_balance ,  NOW()       ,  NULL       ,  fungible_type, comment, 'admin_fabricate');
        RETURN 'done';
    END;
    $$
    LANGUAGE plpgsql;