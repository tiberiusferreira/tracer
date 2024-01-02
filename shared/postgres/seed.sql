create database tracer;
ALTER DATABASE tracer set default_statistics_target = 1000;
alter database tracer set plan_cache_mode = 'force_custom_plan';
alter database tracer set work_mem = '8MB';
CREATE EXTENSION if not exists btree_gin;
-- CREATE TYPE value_type AS ENUM ('string', 'i64', 'f64', 'bool');
CREATE TYPE severity_level AS ENUM ('trace', 'debug', 'info', 'warn', 'error');

create domain identifier as varchar(512)
    CHECK (
        length(trim(value)) > 0
        );
comment on domain identifier is 'Non empty text with limited size of 512 chars';

create domain text_value as varchar(1572864);
comment on domain text_value is 'Text with limited size of 1572864 chars';

create domain ubigint AS bigint
    CHECK (
        value >= 0
        );

comment on domain ubigint is 'Positive Bigint';


create table orphan_event
(
    id           bigserial primary key,
    timestamp    ubigint        not null,
    service_name identifier     not null,
    severity     severity_level not null,
    message      text_value     null
);
create unique index on orphan_event (timestamp, id);
create index on orphan_event (timestamp, service_name, severity);
create index on orphan_event (service_name);


create table orphan_event_key_value
(
    orphan_event_id bigint     not null,
    timestamp       ubigint    not null,
    key             identifier not null,
    value           text_value not null,
    foreign key (timestamp, orphan_event_id) references orphan_event (timestamp, id) on delete cascade,
    primary key (timestamp, key, orphan_event_id)
);
create index on orphan_event_key_value (timestamp, orphan_event_id);


create table trace
(
    service_id           bigint,
    id                   bigint,
    service_name         identifier            not null,
    timestamp            ubigint               not null,
    top_level_span_name  identifier            not null,
    duration             ubigint               null,
    original_span_count  ubigint default 0     not null,
    original_event_count ubigint default 0     not null,
    stored_span_count    ubigint default 0     not null,
    stored_event_count   ubigint default 0     not null,
    event_bytes_count    ubigint default 0     not null,
    warning_count        ubigint default 0     not null,
    has_errors           boolean default false not null,
    updated_at           ubigint               not null,
    primary key (service_id, id)
);
create unique index on trace (timestamp, service_name, top_level_span_name, duration, id, service_id);
create index on trace (warning_count);
create index on trace (has_errors);

create table span
(
    service_id bigint     not null,
    trace_id   bigint     not null,
    id         bigint     not null,
    timestamp  ubigint    not null,
    parent_id  bigint,
    duration   ubigint    null,
    name       identifier not null,
    relocated  boolean    not null,
    foreign key (service_id, trace_id) references trace (service_id, id) on delete cascade,
    primary key (service_id, trace_id, id),
    foreign key (service_id, trace_id, parent_id) references span (service_id, trace_id, id) on delete cascade
);
create index span_trace_id_parent_id on span (service_id, trace_id, parent_id);
comment on index span_trace_id_parent_id is 'For fast deletions';
-- create index span_by_name_and_trace_with_id on span (name, trace_id, service_id);
-- comment on index span_by_name_and_trace_with_id is 'Allows filtering spans by name before joining with trace';


create table span_key_value
(
    service_id bigint     not null,
    trace_id   bigint     not null,
    span_id    bigint     not null,
    timestamp  ubigint    not null,
    key        identifier not null,
    value      text_value not null,
    foreign key (service_id, trace_id, span_id) references span (service_id, trace_id, id) on delete cascade,
    primary key (service_id, trace_id, span_id, key)
);
create index on span_key_value (timestamp, key, trace_id);
create index span_key_value_trace_id_span_id on span_key_value (trace_id, span_id);
comment on index span_key_value_trace_id_span_id is 'For fast deletions';

create table event
(
    service_id bigint         not null,
    trace_id   bigint         not null,
    span_id    bigint         not null,
    id         bigserial      not null,
    timestamp  ubigint        not null,
    message    text_value     null,
    severity   severity_level not null,
    relocated  boolean        not null,
    foreign key (service_id, trace_id, span_id) REFERENCES span (service_id, trace_id, id) on delete cascade,
    primary key (service_id, trace_id, span_id, id)
);
-- CREATE INDEX ON event USING gin (name, service_id, trace_id);

create table event_key_value
(
    service_id bigint     not null,
    trace_id   bigint     not null,
    span_id    bigint     not null,
    event_id   bigint     not null,
    key        identifier not null,
    value      text_value not null,
    timestamp  ubigint    not null,
    foreign key (service_id, trace_id, span_id, event_id) references event (service_id, trace_id, span_id, id) on delete cascade,
    primary key (trace_id, span_id, key, event_id) include (value)
);
create index on event_key_value (key, trace_id);
create index event_key_value_trace_id_span_id_event_id on event_key_value (trace_id, span_id, event_id);
comment on index event_key_value_trace_id_span_id_event_id is 'For fast deletions';


create table service_config
(
    service_name                       identifier not null,
    min_instance_count                 ubigint    not null,
    max_active_trace                   ubigint    not null,
    traces_dropped_per_min             ubigint    not null,
    spe_per_min                        ubigint    not null,
    log_per_min                        ubigint    not null,
    log_dropped_per_min                ubigint    not null,
    events_kb_per_min                  ubigint    not null,
    export_buffer_usage                ubigint    not null,
    max_traces_with_warning_percentage ubigint    not null,
    max_traces_with_error_percentage   ubigint    not null,
    max_trace_duration                 ubigint    not null,
    primary key (service_name)
);

create table service_trace_overwrite
(
    service_name                       identifier not null,
    top_level_span_name                identifier not null,
    max_traces_with_warning_percentage ubigint    not null,
    max_traces_with_error_percentage   ubigint    not null,
    max_trace_duration                 ubigint    not null,
    primary key (service_name, top_level_span_name)
);


/*
create index event_name_client_id_trace_id_idx
    on event using gin (name, client_id, trace_id);
*/
--
--
--
--
--
-- drop table if exists service_traces;
-- drop table if exists service;
-- drop table if exists time_bucket;
-- -- Service stats
-- -- create table time_bucket
-- -- (
-- --     time timestamp primary key,
-- --     check ( extract(minute from time)%5=0 and extract(microsecond from time)=0 )
-- -- );
-- --
-- create table service
-- (
--     time         timestamp  not null,
--     service_name identifier not null,
--     env          identifier not null,
--     primary key (time, service_name)
-- );
--
-- create table service_traces
-- (
--     time                  timestamp  not null,
--     service_name          identifier not null,
--     env                   identifier not null,
--     trace_name            identifier not null,
--     service_uuid          identifier not null,
--
--     total_count           ubigint    not null default 0,
--     span_plus_event_count ubigint    not null default 0,
--     rate_limited_count    ubigint    not null default 0,
--     partial_count         ubigint    not null default 0,
--     orphan_log_count      ubigint    not null default 0,
--     warning_count         ubigint    not null default 0,
--     error_count           ubigint    not null default 0,
--     total_duration        ubigint    not null default 0,
--     max_duration          ubigint    not null default 0,
--     primary key (time, service_name, env, service_uuid),
--     check ( extract(minute from time) % 5 = 0 and extract(microsecond from time) = 0 )
-- );
--

