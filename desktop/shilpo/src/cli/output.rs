use serde::Serialize;
use serde_json::Value;

#[allow(dead_code)]
pub const EXIT_SUCCESS: i32 = 0;
#[allow(dead_code)]
pub const EXIT_FAILURE: i32 = 1;
#[allow(dead_code)]
pub const EXIT_INVALID_ARGS: i32 = 2;
#[allow(dead_code)]
pub const EXIT_UNAVAILABLE: i32 = 3;
#[allow(dead_code)]
pub const EXIT_TIMEOUT: i32 = 4;
#[allow(dead_code)]
pub const EXIT_PROTOCOL_MISMATCH: i32 = 5;
#[allow(dead_code)]
pub const EXIT_AUTH_FAILURE: i32 = 6;
#[allow(dead_code)]
pub const EXIT_INTERNAL_ERROR: i32 = 70;

#[derive(Debug, Clone, Serialize)]
pub struct JsonEnvelope {
    pub schema_version: u32,
    pub ok: bool,
    pub command: String,
    pub data: Value,
    pub warnings: Vec<String>,
    pub error: Option<JsonError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug)]
pub struct CliOutput {
    pub json: bool,
    pub quiet: bool,
}

impl CliOutput {
    pub fn new(json: bool, quiet: bool) -> Result<Self, (i32, String)> {
        if json && quiet {
            return Err((
                EXIT_INVALID_ARGS,
                "error: combining --json and --quiet is invalid usage".into(),
            ));
        }
        Ok(Self { json, quiet })
    }

    pub fn success<T: Serialize>(
        &self,
        command_name: &str,
        data: &T,
        human_msg: Option<&str>,
        warnings: Vec<String>,
    ) -> i32 {
        if self.json {
            let env = JsonEnvelope {
                schema_version: 1,
                ok: true,
                command: command_name.into(),
                data: serde_json::to_value(data).unwrap_or(Value::Null),
                warnings,
                error: None,
            };
            println!("{}", serde_json::to_string(&env).unwrap_or_default());
        } else if !self.quiet {
            if let Some(msg) = human_msg {
                println!("{msg}");
            }
            for warning in warnings {
                eprintln!("warning: {warning}");
            }
        }
        EXIT_SUCCESS
    }

    pub fn error(
        &self,
        command_name: &str,
        code: &str,
        message: &str,
        details: Option<Value>,
        warnings: Vec<String>,
        exit_code: i32,
    ) -> i32 {
        if self.json {
            let env = JsonEnvelope {
                schema_version: 1,
                ok: false,
                command: command_name.into(),
                data: details.clone().unwrap_or(Value::Null),
                warnings,
                error: Some(JsonError {
                    code: code.into(),
                    message: message.into(),
                    details,
                }),
            };
            println!("{}", serde_json::to_string(&env).unwrap_or_default());
        } else {
            if message.starts_with("error")
                || message.contains('\n')
                || message.contains(": error: ")
                || message.contains(": [")
            {
                eprintln!("{message}");
            } else {
                eprintln!("error: {message}");
            }
            for warning in warnings {
                eprintln!("warning: {warning}");
            }
        }
        exit_code
    }
}
