use crate::print_if_dbg;
use std::collections::HashMap;
use tracing::field::{Field, Visit};

pub struct AttributesVisitor {
    pub message: Option<String>,
    pub key_vals: HashMap<String, String>,
}

impl AttributesVisitor {
    pub fn new() -> Self {
        Self {
            message: None,
            key_vals: HashMap::new(),
        }
    }
}

impl Visit for AttributesVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        let context = "record_str";
        let key = field.name();
        print_if_dbg(context, format!("Got {} - {:?}", key, value));
        if key == "message" {
            self.message = Some(value.to_string());
        } else {
            self.key_vals.insert(key.to_string(), value.to_string());
        }
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let context = "record_debug";
        let key = field.name();
        let val = format!("{:?}", value);
        print_if_dbg(context, format!("Got {} - {:?}", key, value));
        if key == "message" {
            self.message = Some(val);
        } else {
            self.key_vals.insert(key.to_string(), val);
        }
    }
}
