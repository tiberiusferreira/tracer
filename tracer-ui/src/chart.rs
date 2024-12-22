use charming::component::{Axis, Legend, LegendType};
use charming::datatype::{CompositeValue, NumericValue};
use charming::element::{
    AxisPointer, AxisPointerAxis, AxisType, Color, ItemStyle, NameLocation, TextStyle, Tooltip,
    Trigger, TriggerOn,
};
use charming::{Chart, WasmRenderer};
use leptos::html::Div;
use leptos::prelude::*;

#[derive(Debug, Clone)]
pub struct StackedBarGraphData {
    pub dom_id_to_render_to: String,
    pub x_axis_label: String,
    pub y_axis_label: String,
    pub series: Vec<CategoryGraphSeries>,
    #[allow(unused)]
    pub click_event_timestamp_receiver: Option<WriteSignal<Option<u64>>>,
}
#[derive(Debug, Clone)]
pub struct CategoryGraphSeries {
    pub name: String,
    pub category_values: Vec<String>,
    pub y_values: Vec<f64>,
}
impl CategoryGraphSeries {
    pub fn new(name: String) -> Self {
        Self {
            name,
            category_values: vec![],
            y_values: vec![],
        }
    }
    pub fn push_data(&mut self, x_value: String, y_value: f64) {
        self.category_values.push(x_value);
        self.y_values.push(y_value);
    }
}

pub fn create_dom_el_ref_and_graph_call_action(
    data: StackedBarGraphData,
    create_chart_action: Action<StackedBarGraphData, ()>,
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

pub fn create_create_chart_action() -> Action<StackedBarGraphData, ()> {
    Action::new(move |graph_data: &StackedBarGraphData| {
        let dom_element_id = graph_data.dom_id_to_render_to.clone();
        let mut graph_data = graph_data.clone();
        async move {
            let mut chart = Chart::new()
                .x_axis(
                    Axis::new()
                        .type_(AxisType::Category)
                        .name_location(NameLocation::Middle)
                        .name_text_style(TextStyle::new().font_size(18.))
                        .name(&graph_data.x_axis_label)
                        .axis_pointer(AxisPointer::new().axis(AxisPointerAxis::X).show(true))
                        .name_gap(30.),
                )
                .y_axis(
                    Axis::new()
                        .type_(AxisType::Value)
                        .name(&graph_data.y_axis_label)
                        .name_text_style(TextStyle::new().font_size(18.))
                        .name_gap(30.)
                        .name_location(NameLocation::Middle),
                )
                .color(vec![
                    Color::Value("rgb(20, 255, 255)".to_string()),
                    Color::Value("rgb(255, 20, 20)".to_string()),
                    Color::Value("rgb(20, 255, 20)".to_string()),
                    Color::Value("rgb(255, 255, 20)".to_string()),
                ])
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
                    charming::series::Bar::new()
                        .stack("total")
                        .item_style(ItemStyle::new().opacity(1.0))
                        .data(
                            series
                                .category_values
                                .iter()
                                .zip(series.y_values.iter())
                                .map(|(x, y)| {
                                    CompositeValue::Array(vec![
                                        CompositeValue::String(x.clone()),
                                        CompositeValue::Number(NumericValue::Float(*y)),
                                    ])
                                })
                                .collect::<Vec<CompositeValue>>(),
                        )
                        .name(&series.name),
                );
            }

            let renderer = WasmRenderer::new(825, 500);
            let chart_instance = renderer
                .render(dom_element_id.to_string().as_str(), &chart)
                .unwrap();
            // let js_value: wasm_bindgen::JsValue = chart_instance.into();
            // let js_string = js_sys::JSON::stringify(&js_value).unwrap();
            //
            // info!(js_string = ?js_string, "value");
            // let value = js_sys::Reflect::get(&js_value, &"series".into()).unwrap();
            // info!(value=?value, "value");
            // js_value.
            // js_value

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
