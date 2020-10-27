alter table item_types add column long_name_plural text;
alter table item_types add column long_name_ambiguous text;
update item_types set long_name_plural = 'Generators' where "name" = 'gen';
update item_types set long_name_plural = 'Capital' where "name" = 'pc';
update item_types set long_name_ambiguous = 'generator(s)' where "name" = 'gen';
update item_types set long_name_ambiguous = 'capital' where "name" = 'pc';
alter table item_types alter column long_name_plural set not null;
alter table item_types alter column long_name_ambiguous set not null;

create table item_type_aliases (
    "name" text not null references item_types("name"),
    alias text primary key
);

insert into item_types ("name", long_name_plural, long_name_ambiguous) VALUES ('sb', 'StatusBucks', 'statusbuck(s)');

--const PC_NAMES :&[&str] = &["pc","politicalcapital","political-capital","capital"];
--const GEN_NAMES:&[&str] = &["gen", "g", "generator", "generators", "gens"];
insert into item_type_aliases ("name", alias) VALUES
    ('pc', 'pc'),
    ('pc', 'politicalcapital'), 
    ('pc', 'political-capital'),
    ('pc', 'capital'),
    ('gen', 'gen'),
    ('gen', 'g'),
    ('gen', 'generator'),
    ('gen', 'generators'),
    ('gen', 'gens'),
    ('sb', 'sb'),
    ('sb', '$b'),
    ('sb', 's$'),
    ('sb', 'statusbucks'),
    ('sb', 'status-bucks'),
    ('sb', 'statusbuck'),
    ('sb', 'status-buck'),
    ('sb', 'status$');