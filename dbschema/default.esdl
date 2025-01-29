module default {
    type Service {
      required env: str { constraint max_len_value(256) };
      required name: str { constraint max_len_value(256) };
      required log_filter: LogFilter { constraint exclusive };
      multi instances := .<service[is ServiceInstance];
      constraint exclusive on ((.env, .name));
    }

    type LogFilter{
        required _value: str { constraint max_len_value(4096) };
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
    type NormalizedAttributeName{
        required _value: str { constraint max_len_value(1024); constraint exclusive; };
    }
    # Attributes are repeated over and over
    type NormalizedAttributeContent{
        required _value: json {
            constraint expression on (
                len(to_str(__subject__)) < 10_000_000
            );
            constraint exclusive;
        };
    }

    type Attribute{
        required normalized_name: NormalizedAttributeName;
        required name := .normalized_name._value;
        required normalized_content: NormalizedAttributeContent;
        required content := .normalized_content._value;
        constraint exclusive on ((.normalized_name, .normalized_content));
    }

    # Span name are repeated on each trace for the most part
    type NormalizedSpanName{
        required _value: str { constraint exclusive; constraint max_len_value(1024) };
    }

    type NormalizedEventMessage{
        required _value: str { constraint exclusive; constraint max_len_value(10_000_000) };
    }

    type Span {
      required span_count_id: int64 { constraint min_value(1) };
      required trace: Trace;
      required normalized_name: NormalizedSpanName;
      required name := .normalized_name._value;
      parent: Span;
      multi events := .<span[is Event];
      required started_at_nanos: int64 {
            constraint min_value(0)
      };
      required duration_nanos: int64 {
            constraint min_value(0)
      };
      required has_ended: bool {
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
      trigger parent_is_from_same_trace after insert, update for each
        when (exists(__new__.parent))
        do (
              assert(
                   __new__.parent.trace.id = __new__.trace.id,
                  message := "span must link to parent from same trace",
              )
      );
      # checks left to the application:
      # there is a single root span which has span_count_id = 1
      # span span_count_id within same trace is contiguous, no gaps
    }

    type Event {
      required span: Span;
      normalized_message: NormalizedEventMessage;
      message := .normalized_message._value;
      required service_instance_update: ServiceInstanceUpdate;
      required timestamp: int64 {
        constraint min_value(0)
      };
      multi attributes: Attribute;
      trigger same_instance_as_trace after insert, update for each do (
        assert(
            __new__.span.trace.service_instance_update.service_instance = __new__.service_instance_update.service_instance,
            message := "event instance update must come from same instance as trace",
        )
      )
    }

    type TimeSeries{
        required name: str { constraint max_len_value(256) };
        required query: str { constraint max_len_value(1024) };
        required look_back_window_seconds: int32 { constraint min_value(5); };
        max_interval_without_data_seconds: int32 { constraint min_value(1); };
        min_value_threshold: int32;
        max_value_threshold: int32;
        constraint exclusive on (.name);
    }

    type TimeSeriesAlertChecks{
        required time_series: TimeSeries;
        alert_message: str { constraint max_len_value(4096) };
        required notification_sent: bool { default :=  false };
        required created_at: datetime { default := datetime_of_statement(); };
    }

    type Dashboard{
        required name: str { constraint max_len_value(256) };
        required multi charts: DashboardChart { constraint exclusive };
    }

    type DashboardChart{
        required name: str { constraint max_len_value(256) };
        required time_series: TimeSeries;
        required y_label: str { constraint max_len_value(256) };
        required _index: int32 { constraint min_value(0) };
    }

#
#
# insert TimeSeries{
#   name:= "dawd",
#   look_back_window_seconds := 60,
#   query := "with
#   start_datetime := <datetime>$start_datetime,
#   end_datetime   := <datetime>$end_datetime,
#   recent_service_updates := (
#     select ServiceInstanceUpdate
#       filter
#         .created_at >= start_datetime and
#         .created_at <= end_datetime
#       order by .created_at desc
#   ),
# recent_service_updates_by_service_name := (
#   group recent_service_updates {
#     created_at,
#     export_buffer_size_bytes
#   }
#   using service_name := .service_instance.service.name
#   by service_name
#   ),
# select recent_service_updates_by_service_name {
#   series_name := .key.service_name,
#   data_points := .elements {
#     date := .created_at,
#     value := .export_buffer_size_bytes
#   },
# }",
#   max_interval_without_data_seconds := 10,
#   min_value_threshold := 0,
#   max_value_threshold := 10_000,
# };
#
#
#
#
};