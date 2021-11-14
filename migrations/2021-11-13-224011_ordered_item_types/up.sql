-- Your SQL goes here
alter table item_types add column "position" int;

update item_types set "position" = 0 where "name" = 'gen';
update item_types set "position" = 1 where "name" = 'pc';
update item_types set "position" = 2 where "name" = 'sb';

alter table item_types alter column "position" set not null;

create sequence item_types_position_seq start with 3 owned by item_types."position";

alter table item_types alter column "position" set default nextval('item_types_position_seq');