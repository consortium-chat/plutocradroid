drop table if exists item_type_aliases;
delete from item_types where "name" = 'sb';
alter table item_types drop column long_name;