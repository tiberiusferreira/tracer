use crate::datetime::secs_since;
use charming::component::{Axis, DataZoom, DataZoomType, Legend, LegendType};
use charming::datatype::{CompositeValue, NumericValue};
use charming::element::{
    AxisPointer, AxisPointerAxis, AxisType, NameLocation, TextStyle, Tooltip, Trigger, TriggerOn,
};
use charming::series::Scatter;
use charming::{Chart, WasmRenderer};
use leptos::html::Div;
use leptos::{create_action, Action, NodeRef, SignalSet, WriteSignal};
use tracing::info;

#[derive(Debug, Clone)]
pub struct GraphSeries {
    pub name: String,
    pub original_x_values: Vec<u64>,
    pub x_values: Vec<f64>,
    pub y_values: Vec<f64>,
}
impl GraphSeries {
    pub fn new(name: String) -> Self {
        Self {
            name,
            original_x_values: vec![],
            x_values: vec![],
            y_values: vec![],
        }
    }
    pub fn push_data(&mut self, timestamp: u64, data: f64) {
        self.original_x_values.push(timestamp);
        let minutes_since = secs_since(timestamp) as f64 / 60.;
        self.x_values.push(minutes_since);
        self.y_values.push(data);
    }
}

#[derive(Debug, Clone)]
pub struct GraphData {
    pub dom_id_to_render_to: String,
    pub y_name: String,
    pub x_name: String,
    pub series: Vec<GraphSeries>,
    pub click_event_timestamp_receiver: Option<WriteSignal<Option<u64>>>,
}

pub fn create_dom_el_ref_and_graph_call_action(
    data: GraphData,
    create_chart_action: Action<GraphData, ()>,
) -> (NodeRef<Div>, String) {
    let active_traces_graph = NodeRef::<Div>::new();
    let dom_id_to_render_to = data.dom_id_to_render_to.clone();
    active_traces_graph.on_load({
        move |_e| {
            create_chart_action.dispatch(data);
        }
    });
    (active_traces_graph, dom_id_to_render_to)
}

pub fn create_create_chart_action() -> Action<GraphData, ()> {
    create_action(move |graph_data: &GraphData| {
        let el_id = graph_data.dom_id_to_render_to.clone();
        let mut graph_data = graph_data.clone();
        async move {
            let mut chart = Chart::new()
                .x_axis(
                    Axis::new()
                        .type_(AxisType::Value)
                        .name_location(NameLocation::Middle)
                        .name_text_style(TextStyle::new().font_size(18.))
                        .name(&graph_data.x_name)
                        .axis_pointer(AxisPointer::new().axis(AxisPointerAxis::X).show(true))
                        .inverse(true)
                        .name_gap(20.),
                )
                .y_axis(
                    Axis::new()
                        .type_(AxisType::Value)
                        .name(&graph_data.y_name)
                        .name_text_style(TextStyle::new().font_size(18.))
                        .name_gap(30.)
                        .name_location(NameLocation::Middle),
                )
                .data_zoom(
                    DataZoom::new()
                        .type_(DataZoomType::Slider)
                        .start_value(0.)
                        .end_value(5.),
                )
                .legend(
                    Legend::new()
                        .data(
                            graph_data
                                .series
                                .iter()
                                .map(|s| s.name.clone())
                                .collect::<Vec<String>>(),
                        )
                        .show(true)
                        .type_(LegendType::Scroll),
                )
                .tooltip(
                    Tooltip::new()
                        .trigger(Trigger::Item)
                        .trigger_on(TriggerOn::MousemoveAndClick),
                );
            for series in &graph_data.series {
                chart = chart.series(
                    Scatter::new()
                        .symbol_size(5.)
                        .data(
                            series
                                .x_values
                                .iter()
                                .zip(series.y_values.iter())
                                .map(|(a, b)| {
                                    CompositeValue::Array(vec![
                                        CompositeValue::Number(NumericValue::Float(*a)),
                                        CompositeValue::Number(NumericValue::Float(*b)),
                                    ])
                                })
                                .collect::<Vec<CompositeValue>>(),
                        )
                        .name(&series.name),
                );
            }

            let renderer = WasmRenderer::new(825, 500);
            let chart_instance = renderer.render(el_id.to_string().as_str(), &chart).unwrap();
            let listener = graph_data.click_event_timestamp_receiver.take();
            let series = graph_data.series;
            WasmRenderer::on_event(&chart_instance, "click", move |c| {
                let timestamp = series[c.series_index].original_x_values[c.data_index];
                info!("Clicked on {:#?}", timestamp);
                info!("{:#?}", c);
                if let Some(l) = listener {
                    l.set(Some(timestamp));
                }
            });
            ()
        }
    })
}
