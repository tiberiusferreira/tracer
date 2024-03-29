use api_structs::ui::trace::chunk::Span;
use leptos::{view, Fragment};
use log::info;
use std::collections::HashMap;

pub fn create_summary_html_span_and_children_single_layer(
    root_start_time_unix_nanos: u64,
    root_duration_nanos: u64,
    spans: &[Span],
    spans_by_parent_id: &HashMap<i64, Vec<Span>>,
    curr_depth: i32,
    html_span_and_children_summary: &mut Vec<Fragment>,
    max_depth: &mut i32,
) {
    if curr_depth > *max_depth {
        *max_depth = curr_depth;
    }
    let empty = vec![];
    let mut next_layer_spans = spans.iter().fold(Vec::new(), |mut acc, curr| {
        acc.extend_from_slice(&spans_by_parent_id.get(&curr.id).unwrap_or(&empty));
        acc
    });
    next_layer_spans.sort_by_key(|k| k.timestamp);

    // group very small children into "single" element
    // let spans = spans
    //     .to_vec()
    //     .into_iter()
    //     .fold(Vec::new(), |mut acc: Vec<Span>, curr| {
    //         let span_duration = curr.duration.unwrap_or(root_duration_nanos);
    //         let percentage_0_to_100 = (100 * span_duration) as f64 / root_duration_nanos as f64;
    //         if percentage_0_to_100 < 0.2 {
    //             if let Some(last) = acc.last_mut() {
    //                 let last_plus_curr_combined_duration = last.duration + span_duration;
    //                 let combined_percentage_0_to_100 = (100 * last_plus_curr_combined_duration)
    //                     as f64
    //                     / root_duration_nanos as f64;
    //                 let last_end_time = last.timestamp + last.duration;
    //                 let gap_micros = curr.timestamp.saturating_sub(last_end_time);
    //                 if combined_percentage_0_to_100 < 0.2 && gap_micros < 1000 {
    //                     last.duration += curr.duration + gap_micros;
    //                     return acc;
    //                 }
    //             }
    //         }
    //         acc.push(curr);
    //         acc
    //     });
    // let mut last_end: u64 = 0;
    for s in spans {
        // dont let them overlap when multiple spans happened at ~ the same time
        // let overlap = last_end.saturating_sub(s.timestamp);
        // if s.duration >= overlap * 2 {
        html_span_and_children_summary.push(create_span_summary_html(
            root_start_time_unix_nanos,
            root_duration_nanos,
            s.timestamp,
            s.duration,
            curr_depth,
            &s.name,
        ));
        // last_end = s.timestamp + s.duration;
        // }
    }
    if !next_layer_spans.is_empty() {
        create_summary_html_span_and_children_single_layer(
            root_start_time_unix_nanos,
            root_duration_nanos,
            &next_layer_spans,
            &spans_by_parent_id,
            curr_depth + 1,
            &mut *html_span_and_children_summary,
            max_depth,
        );
    }
}

fn create_span_summary_html(
    root_start_time_unix_nanos: u64,
    root_duration_nanos: u64,
    start_time_unix_nanos: u64,
    duration_nanos: Option<u64>,
    depth: i32,
    span_name: &str,
) -> Fragment {
    // span may start before the start_timestamp_nanos
    let start_offset_nanos = start_time_unix_nanos.saturating_sub(root_start_time_unix_nanos);
    // info!("\n");
    // info!("start_time_unix_nanos={start_time_unix_nanos}");
    // info!("root_start_time_unix_nanos={root_start_time_unix_nanos}");
    // info!("start_offset_nanos={start_offset_nanos}");
    // let start_offset_nanos = start_time_unix_nanos - root_start_time_unix_nanos;
    let start_offset_percentage = (100 * start_offset_nanos) as f64 / root_duration_nanos as f64;
    info!("start_offset_percentage={start_offset_percentage}");
    let duration_percentage = match duration_nanos {
        None => 100. - start_offset_percentage,
        Some(duration_nanos) => ((100 * duration_nanos) as f64 / root_duration_nanos as f64)
            .max(0.2)
            .min(100f64 - start_offset_percentage),
    };

    let mut depth_to_color: HashMap<i32, String> = HashMap::new();
    depth_to_color.insert(0, "white".to_string());
    depth_to_color.insert(1, "red".to_string());
    depth_to_color.insert(2, "green".to_string());
    depth_to_color.insert(3, "blue".to_string());
    depth_to_color.insert(4, "purple".to_string());
    depth_to_color.insert(5, "brown".to_string());
    depth_to_color.insert(6, "darkred".to_string());
    depth_to_color.insert(7, "forestgreen".to_string());
    let margin_top = 8 + depth * 20; // 8 my "padding"
    let span_style = format!(
        "position: absolute; display: flex; justify-content: center; margin-top: {margin_top}px; height: 15px; background-color: {}; border-radius: 8px",
        depth_to_color.get(&(depth % 8)).unwrap()
    );
    let paragraph = if duration_percentage >= (span_name.len() as f64 / 2.) {
        Some(view! {
            <p style="margin: 0; font-size: x-small; text-align: center">{span_name.to_string()}</p>
        })
    } else {
        None
    };
    let span_html = view! {
        <>
        // <div class="hover-text">hover me
        //     <span class="tooltip-text" id="top">Im a tooltip!</span>
        // </div>
        <div class="summary-span" style={format!("margin-left: {start_offset_percentage}%; width: {duration_percentage}%; {}", span_style)}>
            <span class="tooltip-text" id="top">{span_name.to_string()}</span>
            {paragraph}
        </div>
        </>
    };
    span_html
}
