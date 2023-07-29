create database tracer;
ALTER DATABASE tracer set default_statistics_target = 1000;
alter database tracer set plan_cache_mode = 'force_custom_plan';
alter database tracer set work_mem = '8MB';

create domain identifier as varchar(512)
    CHECK (
        length(trim(value)) > 0
        );
comment on domain identifier is 'Non empty text with limited size of 512 chars';

create domain text_value as varchar(32768)
    CHECK (
        length(trim(value)) > 0
        );
comment on domain text_value is 'Non empty text with limited size of 32768 chars';

create domain ubigint AS bigint
    CHECK (
        value >= 0
        );

comment on domain ubigint is 'Positive Bigint';


create table trace
(
    id                  bigserial primary key,
    timestamp           ubigint    not null,
    service_name        identifier not null,
    top_level_span_name identifier not null,
    duration            ubigint    not null,
    warning_count       ubigint    not null,
    has_errors          boolean    not null
);
create unique index on trace (timestamp, duration, service_name, top_level_span_name, id);
create index on trace (warning_count);
create index on trace (has_errors);

create table span
(
    id        ubigint    not null,
    trace_id  ubigint    not null,
    timestamp ubigint    not null,
    parent_id ubigint,
    duration  ubigint    not null,
    name      identifier not null,
    foreign key (trace_id) references trace (id) on delete cascade,
    primary key (trace_id, id),
    foreign key (trace_id, parent_id) references span (trace_id, id) on delete cascade
);
create index span_by_name_and_trace_with_id on span (name, trace_id);
comment on index span_by_name_and_trace_with_id is 'Allows filtering spans by name before joining with trace';


CREATE TYPE value_type AS ENUM ('string', 'i64', 'f64', 'bool');
CREATE TYPE severity_level AS ENUM ('trace', 'debug', 'info', 'warn', 'error');

create table span_key_value
(
    trace_id       ubigint    not null,
    span_id        ubigint    not null,
    user_generated boolean    not null,
    key            identifier not null,
    value_type     value_type not null,
    value          text_value not null,
    foreign key (trace_id, span_id) references span (trace_id, id) on delete cascade,
    primary key (trace_id, key, span_id) include (value)
);
create index on span_key_value (key, trace_id);


create table event
(
    trace_id  ubigint        not null,
    span_id   ubigint        not null,
    id        ubigint        not null,
    timestamp ubigint        not null,
    name      text_value     not null,
    severity  severity_level not null,
    foreign key (trace_id, span_id) REFERENCES span (trace_id, id) on delete cascade,
    primary key (trace_id, span_id, id)
);
CREATE EXTENSION btree_gin;
CREATE INDEX ON event USING gin (name, trace_id);



create table event_key_value
(
    trace_id       ubigint    not null,
    span_id        ubigint    not null,
    event_id       ubigint    not null,
    user_generated boolean    not null,
    key            identifier not null,
    value_type     value_type not null,
    value          text_value not null,
    foreign key (trace_id, span_id, event_id) references event (trace_id, span_id, id) on delete cascade,
    primary key (trace_id, span_id, key, event_id) include (value)
);
create index on event_key_value (key, trace_id);


drop table if exists service_traces;
drop table if exists service;
drop table if exists time_bucket;
-- Service stats
-- create table time_bucket
-- (
--     time timestamp primary key,
--     check ( extract(minute from time)%5=0 and extract(microsecond from time)=0 )
-- );
--
create table service
(
    time         timestamp  not null,
    service_name identifier not null,
    env          identifier not null,
    primary key (time, service_name)
);

create table service_traces
(
    time                  timestamp  not null,
    service_name          identifier not null,
    env                   identifier not null,
    trace_name            identifier not null,
    service_uuid          identifier not null,

    total_count           ubigint    not null default 0,
    span_plus_event_count ubigint    not null default 0,
    rate_limited_count    ubigint    not null default 0,
    partial_count         ubigint    not null default 0,
    orphan_log_count      ubigint    not null default 0,
    warning_count         ubigint    not null default 0,
    error_count           ubigint    not null default 0,
    total_duration        ubigint    not null default 0,
    max_duration          ubigint    not null default 0,
    primary key (time, service_name, env, service_uuid),
    check ( extract(minute from time) % 5 = 0 and extract(microsecond from time) = 0 )
);


