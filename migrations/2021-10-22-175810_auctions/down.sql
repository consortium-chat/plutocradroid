alter table single drop column last_auto_auction;

alter table single drop column last_task_run;

drop index transfers_current_bid;

drop view balance_history;

alter table transfers
    drop constraint auction_direction_2,
    drop constraint auction_direction_1,
    drop constraint auctions_need_id,
    --we also have to drop and then re-add all constraints that reference transfer_ty :sob:
    drop constraint give_has_both_sides,
    drop constraint motion_matches_ty,

    alter column transfer_ty type text,
    add constraint transfer_ty_enum check (
        transfer_ty IN (
            'motion_create',
            'motion_vote',
            'generated',
            'admin_fabricate',
            'admin_give',
            'give',
            'command_fabricate'
        )
    ),
    drop column auction_id,
    add constraint give_has_both_sides check ((NOT (transfer_ty IN ('give', 'admin_give'))) OR (from_user IS NOT NULL and to_user IS NOT NULL)),
    add constraint motion_matches_ty check ((to_motion IS NOT NULL) = transfer_ty IN ('motion_create', 'motion_vote'))
;

create view balance_history as
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

drop type transfer_type;

drop table auctions;

alter table thing_ids rename to motion_ids;
