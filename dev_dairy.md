# Service Instance <-> Collector communication

One of the design decisions was allowing trace data, mainly spans, to be exported as it is created, as opposed to waiting for the span to fully close.

This opens the door for incomplete data to be exported due to:

- Instance crash
- Network going down for a few minutes and coming back
- Instance being overloaded and not being able to export
- Collector going down and coming back up

In addition, we also allow dynamic sampling, which might ask the Instance to stop collecting data from open traces if they are producing too many spans or events.

## Sampling

There are two kinds of traces:

1. Regular ones, with small total duration
2. Infinite or long duration ones

For 1, we can discard a certain % of them.

For 2, we can discard all data, but the open spans and discard spans as soon as they close until we have budget again.

## Keeping Service Instance memory usage in check

On the export interval (5s or so) all open spans are sent to the Collector along with all closed spans.
The open spans are kept on the Service Instance. Closed spans and all events are removed.

Keeping only the open spans after each export keeps the memory usage in check.

# Unused ideas

## Keeping Buffer under limit while keeping data consistency

- Trace data is referential by means of spans having parents and children.
- Also, the parents are open, running at least as long as any of their children is.

Trace example:

```text
A┌────────────────────────────────────────────────────────────┐
A└────────────────────────────────────────────────────────────┘
                                                              
B┌───────┐   ┌───────────────────────────────┐                 
B└───────┘   └───────────────────────────────┘                 
                                                              
C┌────┐         ┌────────────────────────┐                     
C└────┘         └────────────────────────┘                     
                                                              
D    ┌───┐        ┌────────────────────┐                       
D    └───┘        └────────────────────┘                       
                                                              
E                   ┌────────────────┐                         
E                   └────────────────┘                         
                                                              
F                      ┌───────────┐                           
F                      └───────────┘                           
```

However, if we always keep open spans around and only drop closed spans + events from a given period of time back, we can keep referential integrity. In other words:

- Never deleting open spans
- When deleting, start from a time and
    - delete all closed spans which have (start+duration) before that
    - delete all span events from before that

This is effectively "time slicing" the trace.

We can still go over the buffer limit if we have too many spans open, but this is unlikely to be a concern because this usage is analogous to stack trace usage, so it
will scale with "regular" program memory usage.






