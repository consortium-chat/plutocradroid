alter table motions add column is_super boolean not null default false;

update motions set is_super = false where power = 1;
update motions set is_super = true where power = 2;

alter table motions drop column power;