drop view auction_and_winner;

alter table auctions drop constraint max_bid;
alter table auctions drop column last_timer_bump;
alter table auctions drop column max_bid_amt;
alter table auctions drop column max_bid_user;

alter table auctions alter bid_min type int;
alter table auctions alter offer_amt type int;

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