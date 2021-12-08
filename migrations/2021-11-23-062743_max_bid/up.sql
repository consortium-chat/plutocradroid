drop view auction_and_winner;

alter table auctions add column max_bid_user bigint;
alter table auctions add column max_bid_amt bigint;
alter table auctions add column last_timer_bump timestamptz;
alter table auctions add constraint max_bid check ((max_bid_user is null) = (max_bid_amt is null));

alter table auctions alter bid_min type bigint;
alter table auctions alter offer_amt type bigint;

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
    a.max_bid_user,
    a.max_bid_amt,
    COALESCE(t.happened_at, a.created_at) as last_change,
    t.rowid as transfer_id,
    t.from_user as winner_id,
    t.quantity as winner_bid,
    t.happened_at as bid_at,
    COALESCE(a.last_timer_bump, t.happened_at, a.created_at) as last_timer_bump
  from
    auctions a
  left join lateral
    (select * from transfers where auction_id = a.rowid and transfer_ty = 'auction_reserve' order by happened_at desc limit 1) t
  on true
;

update auctions
  set last_timer_bump = auction_and_winner.last_timer_bump
  from auction_and_winner
  where auction_and_winner.auction_id = auctions.rowid
;

alter table auctions alter last_timer_bump set not null;