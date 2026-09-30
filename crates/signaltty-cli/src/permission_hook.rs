use serde_json::Value;

use crate::client::CliError;

/// Hook hosts parse stdout. A cancellation deliberately yields no verdict.
pub fn print_verdict(result: &Value) -> Result<(), CliError> {
    if let Some(verdict) = result.get("native_verdict").filter(|v| !v.is_null()) {
        println!(
            "{}",
            serde_json::to_string(verdict).map_err(|e| CliError::Io(e.to_string()))?
        );
    }
    Ok(())
}
