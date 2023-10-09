// // use crate::notification_worthy_events::TraceInvalidationCause;
// use crate::proto_generated::opentelemetry::proto::common::v1::any_value::Value;
// use crate::proto_generated::opentelemetry::proto::common::v1::{AnyValue, KeyValue};
// use crate::proto_generated::opentelemetry::proto::resource::v1::Resource;
// use crate::proto_generated::opentelemetry::proto::trace::v1::Span;
//
// pub fn has_errors(span: &Span) -> bool {
//     if let Some(status) = &span.status {
//         // 0 = unset, 1=ok, 2=error
//         if status.code == 2 {
//             return true;
//         }
//     }
//     false
// }
//
// pub fn proto_key_value_to_supported(
//     proto: &KeyValue,
// ) -> Result<SupportedKeyValue, TraceInvalidationCause> {
//     let value = proto
//         .value
//         .as_ref()
//         .ok_or(TraceInvalidationCause::from_cause("Empty Key Value pair"))?;
//     let value = any_value_to_supported_value(value)?;
//     if proto.key.is_empty() {
//         return Err(TraceInvalidationCause::from_cause(
//             "Empty Key from key value pair",
//         ));
//     }
//     if value.value.is_empty() {
//         return Err(TraceInvalidationCause::from_cause(
//             "Empty Value from key value pair",
//         ));
//     }
//     Ok(SupportedKeyValue {
//         key: proto.key.clone(),
//         value,
//     })
// }
// #[derive(Debug, Clone)]
// pub struct SupportedKeyValue {
//     pub key: String,
//     pub value: SupportedValue,
// }
//
// #[derive(Debug, Clone, sqlx::Type)]
// #[sqlx(type_name = "value_type", rename_all = "lowercase")]
// pub enum ValueType {
//     String,
//     Bool,
//     I64,
//     F64,
// }
// #[derive(Debug, Clone)]
// pub struct SupportedValue {
//     pub value_type: ValueType,
//     pub value: String,
// }
// pub fn any_value_to_supported_value(
//     any_value: &AnyValue,
// ) -> Result<SupportedValue, TraceInvalidationCause> {
//     let AnyValue { value: Some(value) } = &any_value else {
//         return Err(TraceInvalidationCause::from_cause(
//             "Empty Key Value pair in trace",
//         ));
//     };
//     let (value_type, value_content) = match value {
//         Value::StringValue(string) => (ValueType::String, string.to_string()),
//         Value::BoolValue(boolean) => (ValueType::Bool, boolean.to_string()),
//         Value::IntValue(int) => (ValueType::I64, int.to_string()),
//         Value::DoubleValue(f64) => (ValueType::F64, f64.to_string()),
//         Value::ArrayValue(_) => {
//             return Err(TraceInvalidationCause::from_cause(
//                 "Unsupported value type: ArrayValue",
//             ));
//         }
//         Value::KvlistValue(_) => {
//             return Err(TraceInvalidationCause::from_cause(
//                 "Unsupported value type: KvlistValue",
//             ));
//         }
//         Value::BytesValue(_) => {
//             return Err(TraceInvalidationCause::from_cause(
//                 "Unsupported value type: BytesValue",
//             ));
//         }
//     };
//     Ok(SupportedValue {
//         value_type,
//         value: value_content,
//     })
// }
//
// pub fn service_name_from_resource(resource: &Resource) -> Option<String> {
//     let service_name_resource = resource
//         .attributes
//         .iter()
//         .find(|attribute| attribute.key == "service.name")?;
//     let value = service_name_resource.value.as_ref()?.value.as_ref()?;
//     let Value::StringValue(service_name) = value else {
//         return None;
//     };
//     Some(service_name.to_string())
// }
