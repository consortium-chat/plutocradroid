alter table motions add column power numeric not null default 0;

update motions set power = 1 where is_super = false;
update motions set power = 2 where is_super = true;

alter table motions drop column is_super;