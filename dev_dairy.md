# Service Instance <-> Collector communication

One of the design decisions was allowing trace data, mainly spans, to be exported as it is created, as opposed to waiting for the span to fully close.

This opens the door for incomplete data to be exported:

#### 1. Instance crash:

- Exports part of a trace
- Instance goes down and never comes back

Result:
Traces with partial data, spans without duration

#### 2. Half-dead instance:

- Exports part of a trace
- Goes down, skipping some exports
- Comes back

#### 3. Overloaded instance:

- Exports part of a trace
- Exporting process can't keep up and drops part of the data

In addition, we also allow dynamic sampling, which might ask the Instance to stop collecting data from open traces if they are producing too many spans or events.

#### 4. Sampling of open traces

- Existing open traces might have gaps in their exported data to comply with budget limits.

Result:
Traces with partial data in all parts:

- Missing beginning
- Missing middle
- Missing end

In any of these cases instances may try to reexport the same data.
Network equipment in the way between may also try to re-send the data, and we might get the same data multiple times.

In summary, we need to handle every possible data being lost in transit or transmitted multiple times.

## Problems it brings

1. We can't know what is still running.
2. For each trace, we can't know if we lost anything or not.
3. The data we get might reference data we don't have.

Ideally we should at least be able to detect lost data and have a minimal amount of information about the trace to still make use of the information we do get.

## What can we do about it and how

We can export a minimal context about the traces still running (even if they have no new data) or that finished between the last export and now.

The context is, for each trace:

- Root Span: <- so we can relocate spans and events to it
- Total Trace (including sampled/dropped data): <- so we can detect if we lost data from this trace by any means
    - Event Count
    - Span Count
- Open Spans: <- so we know where this trace is at

#### Relocation

We can get spans or events with parent that might not exist, because they were lost.

In this case we relocate them to the trace root span, flagging them as relocated.

#### Span Root closed, but active children

In theory spans may be closed in any order, including the root span closing before its children.

However, Tracing Registry only considers a span as closed when there are no more references to it.

So a span won't be closed if it still has children alive.

Summary:
> We always include the trace root span in all exports.

> Relocate spans or events missing its parent to the root span, knowing they might have happened after the root span itself was closed.

> Beware that we might get the same data twice. This should be constant concern given our context reexporting the same data multiple times

### Alternatives

We can't prevent an Instance from dying and leaving incomplete traces.
However, we can make sure it keeps trace state around for as long as needed to export, possibly dropping new traces.

What are we after here? We are after "perfect" trace history, for all the data we get. So if we get any trace data we can be sure the references it makes were previously
exported and saved successfully.

This means we don't need to worry about relocating spans and events anymore and, we don't need to worry about past spans left open without knowing if they were lost or
not, unless the instance dies.

This can be achieved by only marking an export as successful if we get an explicit success response back (not only HTTP 200, but something custom, to indicate our code
saved it successfully).

We would have an internal buffer, hopefully big enough that we don't need to drop anything.

Nevertheless, we need to determine what happens if we hit the buffer capacity.
If the instance loses contact with the collector and then after a long time regains it, what do we expect?

Usually we don't care about old traces, only new ones, so fully dropping them is not a big issue. The issue if for long-running traces that could have started a while ago
and now are still running, but their history has been lost.

# Determinism ideas

Have a random seed that is known and can be injected back:

Mock non-deterministic IO areas:

- Network
- Filesystem
- Database

Use a deterministic executor, or seed tokio using: tokio::runtime::RngSeed



