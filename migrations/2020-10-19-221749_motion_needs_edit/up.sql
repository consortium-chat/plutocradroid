alter table motions add column needs_update boolean not null default false;
create index motions_needs_update on motions(needs_update);