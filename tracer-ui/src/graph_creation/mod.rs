use charming::component::{Axis, Grid};
use charming::datatype::{CompositeValue, NumericValue};
use charming::element::{
    AxisTick, AxisType, Color, ItemStyle, NameLocation, Tooltip, Trigger, TriggerOn,
};
use charming::{Chart, WasmRenderer};
use leptos::html::Div;
use leptos::prelude::*;
#[derive(Debug, Clone)]
pub struct GraphSeries {
    pub name: String,
    // pub original_x_values: Vec<u64>,
    pub x_values: Vec<String>,
    pub y_values: Vec<f64>,
}
impl GraphSeries {
    pub fn new(name: String) -> Self {
        Self {
            name,
            // original_x_values: vec![],
            x_values: vec![],
            y_values: vec![],
        }
    }
    pub fn push_data(&mut self, timestamp: String, data: f64) {
        // self.original_x_values.push(timestamp);

        self.x_values.push(timestamp);
        self.y_values.push(data);
    }
}

#[derive(Debug, Clone)]
pub struct GraphData {
    pub dom_id_to_render_to: String,
    pub y_name: String,
    pub x_name: String,
    pub series: Vec<GraphSeries>,
    #[allow(unused)]
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
    Action::new(move |graph_data: &GraphData| {
        let el_id = graph_data.dom_id_to_render_to.clone();
        let graph_data = graph_data.clone();
        async move {
            let mut chart = Chart::new()
                // .grid(Grid::new())
                .grid(Grid::new().left(30.).right(10.).bottom(30.).top(10.))
                .x_axis(
                    Axis::new()
                        .type_(AxisType::Category)
                        .name_location(NameLocation::Middle), // .name_text_style(TextStyle::new().font_size(18.))
                                                              // .name(&graph_data.x_name)
                                                              // .axis_pointer(AxisPointer::new().axis(AxisPointerAxis::X).show(true)), // .name_gap(20.),
                )
                .y_axis(
                    Axis::new()
                        .type_(AxisType::Value)
                        .name(&graph_data.y_name)
                        .axis_tick(AxisTick::default().split_number(2))
                        // .name_text_style(TextStyle::new().font_size(18.))
                        .name_gap(30.)
                        .name_location(NameLocation::Middle),
                )
                .color(vec![
                    Color::Value("rgb(20, 255, 255)".to_string()),
                    Color::Value("rgb(255, 20, 20)".to_string()),
                    Color::Value("rgb(20, 255, 20)".to_string()),
                    Color::Value("rgb(255, 255, 20)".to_string()),
                ])
                // .legend(
                //     Legend::new()
                //         .data(
                //             graph_data
                //                 .series
                //                 .iter()
                //                 .map(|s| s.name.clone())
                //                 .collect::<Vec<String>>(),
                //         )
                //         .show(true)
                //         .type_(LegendType::Scroll),
                // )
                .tooltip(
                    Tooltip::new()
                        .trigger(Trigger::Item)
                        .trigger_on(TriggerOn::MousemoveAndClick),
                );
            for series in &graph_data.series {
                chart = chart.series(
                    charming::series::Bar::new()
                        .bar_width(18)
                        // .symbol_size(6.5)
                        .item_style(ItemStyle::new().opacity(1.0))
                        .data(
                            series
                                .x_values
                                .iter()
                                .zip(series.y_values.iter())
                                .map(|(a, b)| {
                                    CompositeValue::Array(vec![
                                        CompositeValue::String(a.to_string()),
                                        CompositeValue::Number(NumericValue::Float(*b)),
                                    ])
                                })
                                .collect::<Vec<CompositeValue>>(),
                        )
                        .name(&series.name),
                );
                chart = chart.series(
                    charming::series::Line::new()
                        // .symbol_size(6.5)
                        // .item_style(ItemStyle::new().opacity(1.0))
                        .data(
                            series
                                .x_values
                                .iter()
                                .zip(series.y_values.iter())
                                .map(|(a, _b)| {
                                    CompositeValue::Array(vec![
                                        CompositeValue::String(a.to_string()),
                                        CompositeValue::Number(NumericValue::Float(20.)),
                                    ])
                                })
                                .collect::<Vec<CompositeValue>>(),
                        )
                        .name(format!("{}-min-threshold", &series.name)),
                );
                chart = chart.series(
                    charming::series::Line::new()
                        // .symbol_size(6.5)
                        // .item_style(ItemStyle::new().opacity(1.0))
                        .data(
                            series
                                .x_values
                                .iter()
                                .zip(series.y_values.iter())
                                .map(|(a, _b)| {
                                    CompositeValue::Array(vec![
                                        CompositeValue::String(a.to_string()),
                                        CompositeValue::Number(NumericValue::Float(600.)),
                                    ])
                                })
                                .collect::<Vec<CompositeValue>>(),
                        )
                        .name(format!("{}-max-threshold", &series.name)),
                );
            }
            let el = web_sys::window()
                .unwrap()
                .document()
                .unwrap()
                .get_element_by_id(&el_id)
                .unwrap();
            let width = el.scroll_width();
            let height = el.scroll_height();
            let renderer = WasmRenderer::new(width as u32, height as u32);

            let _chart_instance = renderer.render(el_id.to_string().as_str(), &chart).unwrap();
            // let listener = graph_data.click_event_timestamp_receiver.take();
            // let series = graph_data.series;
            // WasmRenderer::on_event(&chart_instance, "click", move |c| {
            //     let timestamp = series[c.series_index].original_x_values[c.data_index];
            //     info!("Clicked on {:#?}", timestamp);
            //     info!("{:#?}", c);
            //     if let Some(l) = listener {
            //         l.set(Some(timestamp));
            //     }
            // });
            ()
        }
    })
}
