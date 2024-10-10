#[derive(Default)]
pub struct EmbedFieldVisitor {
    pub message: Option<String>,
    pub fields: Vec<(String, String, bool)>,
    pub field_name_prefix: Option<String>,
}

impl EmbedFieldVisitor {
    fn add_field<A: Into<String>, B: Into<String>>(&mut self, name: A, value: B) {
        let prefix = self
            .field_name_prefix
            .clone()
            .unwrap_or_else(|| "".to_owned());
        self.fields
            .push((prefix + &name.into(), value.into(), true));
    }
}

impl tracing::field::Visit for EmbedFieldVisitor {
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.add_field(field.name(), value.to_string());
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.add_field(field.name(), value.to_string());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.add_field(field.name(), value.to_string());
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.add_field(field.name(), value.to_string());
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.add_field(field.name(), value);
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.add_field(field.name(), format!("{:?}", value));
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{:?}", value));
        } else {
            self.add_field(field.name(), format!("{:?}", value));
        };
    }
}
