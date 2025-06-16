use charming::component::{Axis, Grid, Title};
use charming::datatype::{CompositeValue, NumericValue};
use charming::element::{
    AxisLabel, AxisTick, AxisType, Color, Emphasis, Formatter, ItemStyle, JsFunction, NameLocation,
    SplitLine, TextAlign, Tooltip, Trigger, TriggerOn,
};
use charming::{Chart, WasmRenderer};
use js_sys::wasm_bindgen::closure::Closure;
use leptos::html::Div;
use leptos::prelude::*;
use serde_json::json;
use tracing::info;
use web_sys::console::assert;
use web_sys::wasm_bindgen::{JsCast, JsValue};

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
    pub click_event_timestamp_receiver: Option<SignalSetter<u64, LocalStorage>>,
}

pub fn create_dom_el_ref_and_graph_call_action(
    data: GraphData,
    create_chart_action: Action<GraphData, MyEcharts>,
) -> (NodeRef<Div>, String) {
    let active_traces_graph = NodeRef::<Div>::new();
    let dom_id_to_render_to = data.dom_id_to_render_to.clone();
    active_traces_graph.on_load({
        move |_e| {
            create_chart_action.dispatch_local(data);
        }
    });
    (active_traces_graph, dom_id_to_render_to)
}

#[derive(Clone)]
pub struct MyEcharts {
    pub val: JsValue,
}

pub fn create_create_chart_action() -> Action<GraphData, MyEcharts> {
    Action::new_local(move |graph_data: &GraphData| {
        let el_id = graph_data.dom_id_to_render_to.clone();
        let graph_data = graph_data.clone();
        async move {
            let mut chart = Chart::new()
                .grid(Grid::new().left(45.).right(20.).bottom(30.).top(10.))
                .x_axis(
                    Axis::new()
                        .axis_label(AxisLabel::new().formatter(Formatter::Function(
                            JsFunction::new_with_args("value", "return value.substring(0,5)"),
                        )))
                        .type_(AxisType::Category)
                        .name_location(NameLocation::Middle) // .name_text_style(TextStyle::new().font_size(18.))
                        .axis_pointer(
                            charming::element::AxisPointer::new()
                                .axis(charming::element::AxisPointerAxis::X)
                                .show(true),
                        ),
                )
                .y_axis(
                    Axis::new()
                        .type_(AxisType::Value)
                        .name_location(NameLocation::Middle),
                )
                .color(vec![
                    Color::Value("rgb(20, 255, 255)".to_string()),
                    Color::Value("rgb(255, 20, 20)".to_string()),
                    Color::Value("rgb(20, 255, 20)".to_string()),
                    Color::Value("rgb(255, 255, 20)".to_string()),
                ])
                .tooltip(
                    Tooltip::new()
                        .trigger(Trigger::Item)
                        .trigger_on(TriggerOn::MousemoveAndClick),
                );
            for series in &graph_data.series {
                chart = chart.series(
                    charming::series::Bar::new()
                        .bar_width(18)
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

            let chart_instance = renderer.render(el_id.to_string().as_str(), &chart).unwrap();
            // The chart
            let chart_instance_js_value: JsValue = chart_instance.into();
            let chart_instance_js_value_clone: JsValue = chart_instance_js_value.clone();

            let closure = Closure::wrap(Box::new(move |params: JsValue| {
                let params = params.dyn_into::<js_sys::Object>().unwrap();
                let value = js_sys::Reflect::get(&params, &JsValue::from_str("dataIndex")).unwrap();
                let index = value.as_f64().unwrap();
                if let Some(w) = graph_data.click_event_timestamp_receiver {
                    w.set(index as u64);
                }
            }) as Box<dyn FnMut(JsValue)>);
            let js_function = closure.into_js_value();

            let set_option = js_sys::Reflect::get(&chart_instance_js_value, &"setOption".into())
                .expect("Object should have 'setOption' method")
                .dyn_into::<js_sys::Function>()
                .expect("'setOption' should be a function");
            let val = js_sys::JSON::parse(
                r#"{
            "series": [
                {
                    "selectedMode": true,
                    "select": {
                        "itemStyle": {
                          "color": "red"
                        }
                    }
                }
            ]
            }"#,
            )
            .unwrap();
            set_option
                .call1(&chart_instance_js_value, &val)
                .expect("Failed to call 'set_option' method");

            // The `on` method
            let on = js_sys::Reflect::get(&chart_instance_js_value, &"on".into())
                .expect("Object should have 'on' method")
                .dyn_into::<js_sys::Function>()
                .expect("'on' should be a function");

            // The call
            on.call2(&chart_instance_js_value, &"click".into(), &js_function)
                .expect("Failed to call 'on' method");
            std::mem::forget(js_function);

            MyEcharts {
                val: chart_instance_js_value_clone,
            }
        }
    })
}
