module default {
    type Service {
      required env: str { constraint max_len_value(256) };
      required name: str { constraint max_len_value(256) };
      required log_filter: LogFilter { constraint exclusive };
      multi instances := .<service[is ServiceInstance];
      constraint exclusive on ((.env, .name));
    }

    type LogFilter{
        required log_filter: str { constraint max_len_value(4096) };
        required created_at: datetime {
            readonly := true;
            default := datetime_of_statement();
        };
    }

    type ServiceInstance {
      required service: Service;
      required latest_log_filter: LogFilter { constraint exclusive };
      required received_update_count: int64 {
        default := 0;
        constraint min_value(0);
      };
      required registered_at: datetime { default := datetime_of_statement(); };
    }

    type ServiceInstanceUpdate {
      required service_instance: ServiceInstance;
      required export_buffer_size_bytes: int64;
      created_at: datetime { default := datetime_of_statement(); };
    }

    type Trace {
      required trace_count_id: int64{
        annotation description :=
        "incrementing sequence starting from 1 without gaps representing the number of traces produced by the instance"
      };
      multi spans := .<trace[is Span];
      required service_instance_update: ServiceInstanceUpdate;
      required created_at: datetime { default := datetime_of_statement(); };
      trigger trace_count_id_has_no_gaps after insert, update for each
            do (
                with previous_trace_count_id := (
                                        select Trace {
                                            trace_count_id
                                        }
                                         filter .service_instance_update.service_instance=__new__.service_instance_update.service_instance and .id != __new__.id
                                         order by .trace_count_id desc
                                         limit 1
                                    ).trace_count_id,
                expected_new_trace_count_id := (select if exists (previous_trace_count_id) then previous_trace_count_id+1 else 1)
                select assert(
                      __new__.trace_count_id = expected_new_trace_count_id,
                     message := "trace count id is not sequential, trace count ids must be inserted in order expected " ++ to_str(expected_new_trace_count_id) ++ " got " ++ to_str(__new__.trace_count_id),
                )
        );
    }

    # Attributes are repeated over and over
    type AttributeName{
        required name: str { constraint max_len_value(1024) };
    }
    # Attributes are repeated over and over
    type AttributeContent{
        required content: json { constraint max_len_value(10_000_000) };
    }

    type Attribute{
        required name: AttributeName;
        required content: AttributeContent;
    }

    # Span name are repeated on each trace for the most part
    type SpanName{
        required name: str { constraint max_len_value(1024) };
    }

    type EventMessage{
        required message: str { constraint max_len_value(10_000_000) };
    }

    type Span {
      required span_count_id: int64 { constraint min_value(1) };
      required trace: Trace;
      required name: SpanName;
      parent: Span;
      required duration_nanos: int64 {
            constraint min_value(0)
      };
      required is_closed: bool {
        default := false
      };
      multi attributes: Attribute;
      required instance_update: ServiceInstanceUpdate;
      trigger same_instance_as_trace after insert, update for each do (
        assert(
            __new__.trace.service_instance_update.service_instance = __new__.instance_update.service_instance,
            message := "span instance update must come from same instance as trace",
        )
      );
      trigger root_span_is_first_span after insert, update for each
              when (not exists(__new__.parent))
              do (
                    assert(
                         __new__.span_count_id = 1,
                        message := "root span span_count_id must be 1, its the first span",
                    )
            );
      trigger span_count_id_has_no_gaps after insert, update for each
              when (true)
              do (
                    with previous_span_count_id := ((select Span{
                                                                span_count_id
                                                              }
                                                              filter .trace.id=__new__.trace.id and .id!=__new__.id
                                                              order by .span_count_id desc
                                                              limit 1).span_count_id),
                    expected_new_span_count_id := (select if exists (previous_span_count_id) then previous_span_count_id+1 else 1)
                    select assert(
                         __new__.span_count_id = expected_new_span_count_id,
                         message := "span count id is not sequential, span count ids must be inserted in order expected " ++ to_str(expected_new_span_count_id) ++ " got " ++ to_str(__new__.span_count_id),

                    )
            );
      trigger parent_is_from_same_trace after insert, update for each
        when (exists(__new__.parent))
        do (
              assert(
                   __new__.parent.trace.id = __new__.trace.id,
                  message := "span must link to parent from same trace",
              )
      );
      trigger trace_has_single_root after insert, update for each
      when (not exists __new__.parent)
      do (
        # if this a root span (no parent), there must be no existing root span
        assert_single(
            (select Span filter not exists Span.parent),
            message := "a trace can not have more than one root span",
        )
      );
    }

    type Event {
      required span: Span;
      message: EventMessage;
      required service_instance_update: ServiceInstanceUpdate;
      multi attributes: Attribute;
      trigger same_instance_as_trace after insert, update for each do (
        assert(
            __new__.span.trace.service_instance_update.service_instance = __new__.service_instance_update.service_instance,
            message := "event instance update must come from same instance as trace",
        )
      )
    }




};