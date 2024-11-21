
## Problems with Open Telemetry

#### Spans are only exported when closed

Spans are exported in ***reverse order*** and ***only when finished***: https://github.com/open-telemetry/opentelemetry-specification/issues/373

This makes us always see the past state of a system. 

We see what is _currently_ being executed, making it much harder to debug application stuck situations. 

This also means we don't see the root span until the whole trace is exported, and the root span usually has the most important information such as the Http Path, Headers, Body and Status Code.

"Infinite" traces from background workers or event listeners are not well supported, their root span is never exported.

#### Traces shouldn't be the root information

Otel also treats Traces as the "root" information, being uniquely identified by an UUID. This makes it hard to get feedback from the instance itself about how things are going: export buffer usage, instance log level etc.

Tracer however treats Traces as entities that come from a service instance and can't be uniquely identified outside one.

Service instance need to be first registered before they can export traces. 

The registration phase is important for the auto configuration of the instance, such as log levels and whatever else is needed. We can send the instance id back as part of the registration, then we can control the instance IDs better.

During startup the traces and logs can be buffered for after registration.

## Problems with Visualization Tools

## Focus on Spans Tags

We can't query, search, graph or alert by Trace event messages or tags.

## Problems with Tracing Env Filter

We can't set different span and event log levels. So the root span can be filtered out, leading to weird traces.

We can use https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/fn.dynamic_filter_fn.html to do what we want.

## Sampling and How to Deal With Full Buffer


#### Second guessing data

We want traces and logs to be as reliable as the ones written stdout or a local file. We don't want to second guess if we have missing information because the trace was sampled, dropped or corrupt.

#### Deriving Metrics

Traces are used to derive metrics, which won't be reliable if there is missing data.
In this sense, Tracer take the unconventional approach of ***not sampling at all***. 

#### Managing Data Volume

Users are encouraged to use ***Runtime Log Filters*** to adjust the volume of data being generated and monitor the ***Export Buffer Usage*** of each instance to make sure it's not generating more data than can be exported.

The main reason the ***Export Buffer*** full is due to connection issues with Tracer. When this happens, Tracer assumes the instance is dead.

An instance unaccounted for in Tracer and without log to explain its behavior is not a healthy one. For this reason when the Export Buffer gets filled, the whole service will get blocked until it can export data and make room in its buffer.

This also acts as a form of back pressure for cases when the instance is generating logs faster than they can be exported and can be monitored by the ***Export Buffer Usage*** metric.


## Exported Data

Timestamps are always in nanoseconds.

```
struct TraceState {  
    id: u32,  
    // the root span exists and has id=0
    spans: HashMap<u32, Span>,
    new_events: Vec<SpanEvent>,  
}

struct Span {  
    id: u32,  
    name: String,  
    timestamp: u64,
    // duration so far, span might still be open
    duration: u64,  
    parent_id: Option<u32>,  
    key_vals: HashMap<String, String>,  
    location: Location,
    // for representing spans happening as a consequence of another
    // trace. Ex: UI -> to Backend. Backed span may link UI one.
    links_to: Option<GlobalSpanId>,
    // indicates if the span is fully closed, meaning the duration is final
    closed: bool,  
}

pub struct GlobalSpanId {  
	// serialized as a string
    instance_id: uuid::Uuid,  
    trace_id: u32,  
    span_id: u32,  
}

struct SpanEvent {  
    span_id: u32,  
    message: Option<String>,  
    timestamp: u64,  
    severity: Severity,  
    key_vals: HashMap<String, String>,  
    location: Location,  
}

struct Location {  
    module: Option<String>,  
    filename: Option<String>,  
    line: Option<u32>,  
}

// serialized as lowercase
enum Severity {  
    Trace,  
    Debug,  
    Info,  
    Warn,  
    Error
}

```



# Overall Architecture


The hierarchy of entities being:

- Service
	- Has an env: Dev, Stage, Production or a Custom name
	- Has a name: Frontend, Billing, Delivery etc
	- A service can have zero or more instances
- Instance
	- Represents a deployed piece of code
	- Exports telemetry data: traces and logs
- Trace
	- Represents an execution flow



# Instance Exported Data

Instances exports two types of data:

## Instance Statistics

- CPU profiles
- Memory profiles


## Traces

Traces are the primary data exported and represent an execution path.

Designed to allow streaming from single events to spans, even if they are still open. This is useful for tracking long running or stuck execution paths (be it a request or a background job).


Traces are uniquely identified by the combination of:
1. Instance Id
	1. Type: UUIDv7 
	2. Generated by the instance
	3. Globally identifies the instance among all others
2. Trace Id
	1. Type: u32
	2. Sequencial number, without gaps, generated by the instance starting from 0
	3. Uniquely identifies the trace _within_ the instance

Traces consist of spans. 

### Spans

Spans represent a segment of the execution path, usually a function call.

Spans are uniquely identified by a Span Id which is comprised of:
1. Span Id:
	1. Type: u32
	2. Sequencial number, without gaps, generated by the instance starting from 0 for each trace
	3. Uniquely identifies the span _within_ a trace

Additionally, each span has a name and may contain attributes as Key-Values pairs.

Keys may be seen as object path with . (dot) as separator. Example:
````http.response.status_code```` = 200
````http.response.method```` = "POST"
`````url.full````` = "https://www.foo.bar/search?q=OpenTelemetry#SemConv"
See: https://opentelemetry.io/docs/specs/semconv/url/url/

Keys can not be null, nor can values, see: https://opentelemetry.io/docs/specs/otel/common/

Spans can contain events.

#### Spans Links

Spans can link to other span from other traces.

### Events

Events represent an event in the execution path.

Events are composed of:
1. Optional message
2. Optional Key-Value pairs, just like spans

## Linked Traces

Spans can be linked to a span from ***another*** trace by setting a key-value pair:

```tracer.links_to.instance_uuid=0192ad23-0b92-7d3f-9bfe-2e6b8f0a6954```
```tracer.links_to.trace_id=32```
```tracer.links_to.span_id=2```


## Trace Search

Traces can be narrowed down by:
1. Service - exact search with autocomplete
2. A timespan
3. Name - exact search with autocomplete
	1. Note that span names are static and for HTTP servers all traces may end up with "http_request" or similar name, since they are all created in the same function
4. Trace-Level Key Value - exact search with autocomplete
	1. Endpoint (url.path)
	2. Status Code (http.response.status_code)
	3. Query (url.query)
	4. Custom user key-values
6. Duration
7. Warning count
8. Has Error
9. Total Size (Kb) - great than or lower than, showing min - max range 

Traces can also be Deep Searched, which is a slower search looking at their individual spans and events.
1. Span name 
2. Span key-value
3. Event message
4. Event key-value





