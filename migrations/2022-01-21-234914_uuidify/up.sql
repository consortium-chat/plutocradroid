-- MUST HAVE `pgcrypto` module for postgres
--   run `create extension pgcrypto;` as superuser
-- Changing all the primary keys across tables linked with foreign keys... easy right?

--  public | auction_and_winner         | view     | shelvacu
--  public | auctions                   | table    | shelvacu
--  public | balance_history            | view     | shelvacu
--  public | item_type_aliases          | table    | shelvacu
--  public | item_types                 | table    | shelvacu
--  public | item_types_position_seq    | sequence | shelvacu
--  public | motion_ids_rowid_seq       | sequence | shelvacu
--  public | motion_votes               | table    | shelvacu
--  public | motions                    | table    | shelvacu
--  public | single                     | table    | shelvacu
--  public | thing_ids                  | table    | shelvacu
--  public | transfers                  | table    | shelvacu
--  public | transfers_rowid_seq        | sequence | shelvacu

drop view auction_and_winner;
drop view balance_history;

alter table auctions add column id uuid;
alter table item_types add column id uuid;
-- doesnt make sense for motion votes or item type aliases to have uuids
alter table motions add column id uuid;
alter table transfers add column id uuid;

update auctions set id = gen_random_uuid();
update item_types set id = gen_random_uuid();
update motions set id = gen_random_uuid();
update transfers set id = gen_random_uuid();

alter table auctions alter column id set not null;
alter table item_types alter column id set not null;
alter table motions alter column id set not null;
alter table transfers alter column id set not null;

alter table auctions rename column rowid to thing_id;
alter table motions rename column rowid to thing_id;
create index on auctions(thing_id);
create index on motions(thing_id);

alter table auctions rename column offer_ty to offer_ty_old;
alter table auctions add column offer_ty uuid;
update auctions a set a.offer_ty = it.id from item_types it where it."name" = a.offer_ty_old;
alter table auctions alter column offer_ty set not null;
alter table auctions drop column offer_ty_old;

alter table auctions rename column bid_ty to bid_ty_old;
alter table auctions add column bid_ty uuid;
update auctions a set a.bid_ty = it.id from item_types it where it."name" = a.bid_ty_old;
alter table auctions alter column bid_ty set not null;
alter table auctions drop column bid_ty_old;

alter table item_type_aliases add column item_type_id uuid;
update item_type_aliases set item_type_id = it.id from item_types it where it."name" = "name";
alter table item_type_aliases alter column item_type_id set not null;
alter table item_type_aliases drop column "name";

alter table motion_votes add column motion_id uuid;
update motion_votes mv set mv.motion_id = m.id from motions m where m.rowid = mv.motion;
alter table motion_votes alter column motion_id set not null;
-- can't drop it just yet, motion is part of the pkey

alter table transfers rename column auction_id to auction_id__old;
alter table transfers add column auction_id uuid;
update transfers t set t.auction_id = a.id from auctions a where t.auction_id_old = a.thing_id;
alter table transfers drop constraint auctions_need_id;
alter table transfers add constraint auctions_need_id check (
    transfer_ty not in ('auction_create', 'auction_reserve', 'auction_refund', 'auction_payout') or auction_id is not null
);
alter table transfers drop column auction_id_old;

alter table transfers rename column ty to ty__old
alter table transfers add column ty uuid;
update transfers set ty = it.id from item_types it where it."name" = ty__old;
alter table transfers alter column ty set not null;
alter table transfers drop column ty__old;


alter table auctions drop constraint auctions_pkey;
-- keep the column (now thing_id)
alter table auctions add primary key id;

alter table item_types drop constraint item_types_pkey;
-- keep the column (still name)
alter table item_types add primary key id;

alter table motion_votes drop constraint motion_vote_pkey;
alter table motion_votes drop column motion;
alter table motion_votes add primary key (motion_id, "user");

alter table motions drop constraint motions_pkey;
-- keep the column (now thing_id)
alter table motions add primary key id;

alter table transfers drop constraint transfers_pkey;
alter table transfers drop column rowid;
alter table transfers add primary key id;


create type account_type as enum (
    'user',
    'auction',
    'offer'
);

create table users (
    id uuid not null primary key,
    common_name citext unique, --remember, only unique among non-null values.
    discord_id bigint,
    dummy_true_value boolean not null default true check(dummy_true_value)
);

create table wallets (
    id uuid not null primary key,
    ty account_type not null,
    receive_enabled boolean not null, -- can people $give to this account
    owner_user uuid references users(id),
    is_primary boolean not null,
    owner_auction uuid references auctions(id),
    constraint auction_wallets_never_primary check (owner_auction is null OR NOT is_primary),
    constraint exactly_one_owner check (
        (
            (owner_user is not null)::int +
            (owner_auction is not null)::int
        ) = 1
    )
);

create index by_owner_uuid on wallets((coalesce(owner_user, owner_auction)));
create unique index only_one_primary_per_user on wallets(owner_user) where is_primary and owner_user is not null;
create unique index only_one_primary_per_auction on wallets(owner_auction) where is_primary and owner_auction is not null;
create unique index foreign_key_hacks_user on wallets(id, owner_user, is_primary);
create unique index foreign_key_hacks_auction on wallets(id, owner_auction);

-- pub static KNOWN_NAMES: phf::Map<u64, &'static str> = phf::phf_map! {
--     125003180219170816u64 => "Colin",
--     155438323354042368u64 => "Ben",
--     165858230327574528u64 => "Shelvacu",
--     175691653770641409u64 => "DDR",
--     173650493145350145u64 => "Sparks",
--     182663630280589312u64 => "Azure",
--     189620154122895360u64 => "Leeli",
--     240939050360504320u64 => "InvisiBrony",
--     373610438560317441u64 => "Matt",
-- };
insert into users(id, common_name, discord_id) values 
    (gen_random_uuid(), 'Colin',        125003180219170816),
    (gen_random_uuid(), 'Ben',          155438323354042368),
    --('1eea4b9a-0514-5c20-abdb-2b37d19cae51', 'Shelvacu', 165858230327574528),
    (gen_random_uuid(), 'DDR',          175691653770641409),
    (gen_random_uuid(), 'Sparks',       173650493145350145),
    (gen_random_uuid(), 'Azure',        182663630280589312),
    (gen_random_uuid(), 'Leeli',        189620154122895360),
    (gen_random_uuid(), 'InvisiBrony',  240939050360504320),
    (gen_random_uuid(), 'Matt',         373610438560317441)
;
insert into users(id, discord_id)
    select gen_random_uuid(), t.discord_id
    from (
        select auctioneer as discord_id from auctions
        union
        select motioned_by as discord_id from motions
        union
        select "user" as discord_id from motion_votes
        union
        select from_user as discord_id from transfers where from_user is not null
        union
        select to_user as discord_id from transfers where to_user is not null
    ) t
on conflict do nothing;
create index on users(discord_id);

insert into wallets(id, ty, receive_enabled, owner_user, is_primary)
    select gen_random_uuid(), 'user', true, id, true
    from users
;

alter table users add column primary_wallet uuid;
update users u set u.primary_wallet = w.id from wallets w where w.owner_user = u.id;
alter table users alter column primary_wallet set not null;
alter table users add foreign key (primary_wallet, id, dummy_true_value) references wallets(id, owner_user, is_primary) deferrable initially deferred;

-- alter table transfers add column from_wallet uuid;
-- alter table transfers add column to_wallet uuid;

-- update transfers t set t.from_wallet = u.primary_wallet from users u where u.discord_id = t.from_user;
-- update transfers t set t.to_wallet = u.primary_wallet from users u where u.discord_id = t.to_user;

alter type transfer_type add value 'wallet_fungible_chain_start';

alter table transfers rename to transfers__old;
create table transfers__new (
    id uuid primary key,
    fungible_ty text not null references item_types("name"),
    quantity bigint,
    happened_at timestamptz not null,
    ordering int not null, --for keeping transactions with the same timestamp in order, smaller first
    transfer_ty transfer_type not null,
    
    f_wallet uuid,
    f_balance_before bigint,
    f_balance_after bigint,
    f_previous_transaction_f uuid,
    f_previous_transaction_t uuid,
    f_previous_transaction_happened_at timestamptz,
    f_previous_transaction_ordering int,

    t_wallet uuid,
    t_balance_before bigint,
    t_balance_after bigint,
    t_previous_transaction_f uuid,
    t_previous_transaction_t uuid,
    t_previous_transaction_happened_at timestamptz,
    t_previous_transaction_ordering int,

    message_id bigint,
    to_motion uuid references motions(id),
    to_motion_vote_count bigint,
    comment text,
    auction uuid references auctions(id),

    constraint math_f check (
        f_wallet is null
        OR
        f_balance_before is null
        OR
        f_balance_before - quantity = f_balance_after
    ),
    constraint math_f check (
        t_wallet is null
        OR
        t_balance_before is null
        OR
        t_balance_before + quantity = t_balance_after
    ),
    constraint auctions_need_id check (
        transfer_ty not in ('auction_create', 'auction_reserve', 'auction_refund','auction_payout')
        OR
        auction is not null
    ),
    constraint motion_ty_has_motion check (
        transfer_ty not in ('motion_create', 'motion_vote')
        OR
        motion is not null
    ),
    constraint is_motion check (to_motion is null = to_motion_vote_count is null),
    constraint give_has_both_sides check (
        transfer_ty not in ('give', 'admin_give')
        or
        (f_wallet is not null and t_wallet is not null)
    ),
    constraint fabricate_ty_is_sourceless check (
        transfer_ty not in ('admin_fabricate', 'command_fabricate')
        or
        f_wallet is null
    ),
    constraint generated_ty check (
        transfer_ty != 'generated'
        or
        (f_wallet is null AND fungible_ty = 'gen')
    )
    constraint wallet_start check (
        transfer_ty != 'wallet_fungible_chain_start'
        OR
        (
            t_wallet is null
            AND
            f_wallet is not null
            AND
            f_balance_before is null
            AND
            f_balance_after = 0
            AND
            f_previous_transaction_f is null
            AND
            f_previous_transaction_t is null
            AND
            f_previous_transaction_happened_at is null
            AND
            f_previous_transaction_ordering is null
        )
    )
    constraint f_has_balance_before check (f_wallet is null or f_balance_before is not null or transfer_ty = 'wallet_fungible_chain_start'),
    constraint t_has_balance_before check (t_wallet is null or t_balance_before is not null),
    constraint f_null_together check (
        f_wallet is null = f_balance_after is null
        and
        f_wallet is null = (f_previous_transaction_f is null AND f_previous_transaction_t is null)
        and
        f_wallet is null = f_previous_transaction_happened_at is null
        and
        f_wallet is null = f_previous_transaction_ordering is null
    ),
    constraint t_null_together check (
        t_wallet is null = t_balance_after is null
        and
        t_wallet is null = (t_previous_transaction_f is null AND t_previous_transaction_t is null)
        and
        t_wallet is null = t_previous_transaction_happened_at is null
        and
        t_wallet is null = t_previous_transaction_ordering is null
    ),
    constraint f_not_linked_to_two_transfers check (f_previous_transaction_f is null or f_previous_transaction_t is null),
    constraint t_not_linked_to_two_transfers check (t_previous_transaction_f is null or t_previous_transaction_t is null),
    constraint makes_sense check(f_wallet is not null or t_wallet is not null),
    constraint cannot_self_transfer check(f_wallet is null or t_wallet is null or f_wallet != t_wallet),

    foreign key (
        fungible_ty,
        f_previous_transaction_f,
        f_previous_transaction_happened_at,
        f_previous_transaction_ordering,
        f_balance_before,
        f_wallet
    ) references transfers__new(
        fungible_ty,
        id,
        happened_at,
        ordering,
        f_balance_after,
        f_wallet
    ),
    foreign key (
        fungible_ty,
        f_previous_transaction_t,
        f_previous_transaction_happened_at,
        f_previous_transaction_ordering,
        f_balance_before,
        f_wallet
    ) references transfers__new(
        fungible_ty,
        id,
        happened_at,
        ordering,
        t_balance_after,
        t_wallet
    ),
    foreign key (
        fungible_ty,
        t_previous_transaction_f,
        t_previous_transaction_happened_at,
        t_previous_transaction_ordering,
        t_balance_before,
        t_wallet
    ) references transfers__new(
        fungible_ty,
        id,
        happened_at,
        ordering,
        f_balance_after,
        f_wallet
    ),
    foreign key (
        fungible_ty,
        t_previous_transaction_t,
        t_previous_transaction_happened_at,
        t_previous_transaction_ordering,
        t_balance_before,
        t_wallet
    ) references transfers__new(
        fungible_ty,
        id,
        happened_at,
        ordering,
        t_balance_after,
        t_wallet
    ),
);

create unique index single_correct_order on transfers__new(happened_at, ordering);
create unique index consistent_chain_f on transfers__new((coalesce(f_previous_transaction_f, t_previous_transaction_f)));
create unique index consistent_chain_t on transfers__new((coalesce(f_previous_transaction_t, t_previous_transaction_t)));
create unique index foreign_key_magic_1 on transfers__new(fungible_ty,id,happened_at,ordering,f_balance_after,f_wallet);
create unique index foreign_key_magic_2 on transfers__new(fungible_ty,id,happened_at,ordering,t_balance_after,t_wallet);

--create unique index auction_single_payout
create index transfers_current_bid on transfers__new(auction_id, happened_at, ordering) where auction_id is not null AND transfer_ty = 'auction_reserve';
