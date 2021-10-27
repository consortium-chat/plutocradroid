alter table motion_ids rename to thing_ids;

create table auctions (
    rowid bigint primary key references thing_ids(rowid),
    created_at timestamptz not null,
    auctioneer bigint, --if null, then the CONsortium is the auctioneer (fabricated fungibles)
    offer_ty text not null references item_types("name"),
    offer_amt int not null,
    bid_ty text not null references item_types("name"),
    bid_min int not null,
    finished boolean not null default false
);

create type transfer_type as enum (
    'motion_create',
    'motion_vote',
    'generated',
    'admin_fabricate',
    'admin_give',
    'give',
    'command_fabricate',
    --new
    'auction_create', --you've offered up some fungibles for bid
    'auction_reserve', --placing a bid, fungibles are held
    'auction_refund', --someone else outbid you, held fungibles are returned
    'auction_payout' --you've won the auction, and receive the fungibles offered
);

--The type of transfer_ty can't be changed while this view exists, so we kill and re-create it.

drop view balance_history;

alter table transfers
    add column auction_id bigint references auctions(rowid),
    drop constraint transfer_ty_enum,
    --we also have to drop and then re-add all constraints that reference transfer_ty :sob:
    drop constraint give_has_both_sides,
    drop constraint motion_matches_ty,

    alter column transfer_ty type transfer_type using transfer_ty::transfer_type,
    add constraint auctions_need_id check (
        transfer_ty not in ('auction_create', 'auction_reserve', 'auction_refund') or auction_id is not null
    ),
    add constraint auction_direction_1 check (
        transfer_ty not in ('auction_create', 'auction_reserve') or (from_user is not null and to_user is null)
    ),
    add constraint auction_direction_2 check (
        transfer_ty != ('auction_refund') or (from_user is null and to_user is not null)
    ),
    add constraint give_has_both_sides check ((NOT (transfer_ty IN ('give', 'admin_give'))) OR (from_user IS NOT NULL and to_user IS NOT NULL)),
    add constraint motion_matches_ty check ((to_motion IS NOT NULL) = transfer_ty IN ('motion_create', 'motion_vote'))
;

create view auction_and_winner as
  select
    a.rowid as auction_id,
    a.created_at,
    a.auctioneer,
    a.offer_ty,
    a.offer_amt,
    a.bid_ty,
    a.bid_min,
    a.finished,
    COALESCE(t.happened_at, a.created_at) as last_change,
    t.rowid as transfer_id,
    t.from_user as winner_id,
    t.quantity as winner_bid,
    t.happened_at as bid_at
  from
    auctions a
  left join lateral
    (select * from transfers where auction_id = a.rowid and transfer_ty = 'auction_reserve' order by happened_at desc limit 1) t
  on true
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
    "transfer_ty",
    "auction_id"
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
    "transfer_ty",
    "auction_id"
  from transfers
  where
    "to_user" is not null
;

-- We find the current bid by looking for the most recent auction_reserve transaction for that auction
create index transfers_current_bid on transfers(auction_id, happened_at) where auction_id is not null and transfer_ty = 'auction_reserve';

-- An extra safety precaution to make sure an auction can never pay out twice
create unique index auction_single_payout on transfers(auction_id) where transfer_ty = 'auction_payout';

alter table single add column last_task_run timestamptz not null default '2020-03-26T17:03:34 -0700';

--if null then auto auctions are disabled
alter table single add column last_auto_auction timestamptz;