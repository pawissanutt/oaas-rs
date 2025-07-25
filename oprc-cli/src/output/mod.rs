use serde_json::Value;

// Re-export OutputFormat for convenience
pub use crate::types::OutputFormat;

/// Output formatting interface
pub trait Formatter {
    fn format(&self, data: &Value) -> anyhow::Result<String>;
}

pub struct JsonFormatter;
pub struct YamlFormatter;
pub struct TableFormatter;

impl Formatter for JsonFormatter {
    fn format(&self, data: &Value) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(data)?)
    }
}

impl Formatter for YamlFormatter {
    fn format(&self, data: &Value) -> anyhow::Result<String> {
        Ok(serde_yaml::to_string(data)?)
    }
}

impl Formatter for TableFormatter {
    fn format(&self, data: &Value) -> anyhow::Result<String> {
        match data {
            Value::Array(items) => {
                if items.is_empty() {
                    return Ok("No data available".to_string());
                }

                // Extract headers from first item
                let mut headers = Vec::new();
                if let Some(Value::Object(first)) = items.first() {
                    headers = first.keys().cloned().collect();
                    headers.sort();
                }

                let mut table = String::new();

                // Header row
                table.push_str(&format!("{}\n", headers.join(" | ")));
                table.push_str(&format!(
                    "{}\n",
                    "-".repeat(headers.join(" | ").len())
                ));

                // Data rows
                for item in items {
                    if let Value::Object(obj) = item {
                        let row: Vec<String> = headers
                            .iter()
                            .map(|h| {
                                format_value(obj.get(h).unwrap_or(&Value::Null))
                            })
                            .collect();
                        table.push_str(&format!("{}\n", row.join(" | ")));
                    }
                }

                Ok(table)
            }
            Value::Object(obj) => {
                let mut table = String::new();
                table.push_str("Key | Value\n");
                table.push_str("--- | -----\n");

                let mut keys: Vec<_> = obj.keys().collect();
                keys.sort();

                for key in keys {
                    let value =
                        format_value(obj.get(key).unwrap_or(&Value::Null));
                    table.push_str(&format!("{} | {}\n", key, value));
                }

                Ok(table)
            }
            _ => Ok(format!("{}", data)),
        }
    }
}

fn format_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        _ => value.to_string(),
    }
}

/// Get formatter for the specified output format
pub fn get_formatter(format: &OutputFormat) -> Box<dyn Formatter> {
    match format {
        OutputFormat::Json => Box::new(JsonFormatter),
        OutputFormat::Yaml => Box::new(YamlFormatter),
        OutputFormat::Table => Box::new(TableFormatter),
    }
}

/// Format and print data in the specified format
pub fn print_output(data: &Value, format: &OutputFormat) -> anyhow::Result<()> {
    let formatter = get_formatter(format);
    let output = formatter.format(data)?;
    println!("{}", output);
    Ok(())
}

/// Global output arguments that can be added to any command
#[derive(clap::Args, Clone, Debug)]
pub struct OutputArgs {
    /// Output format
    #[arg(short = 'o', long, value_enum, default_value = "json")]
    pub output: OutputFormat,
}
