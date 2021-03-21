create table users (
    id bigserial primary key
    , internal_id bigserial not null
    , one varchar
    , two varchar
);

create unique index users_internal_id on users (internal_id);
