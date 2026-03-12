//! `config/mod.rs`
//!
//! Provides embedded access to the system module definitions.

pub const MODULES_JSON: &str = include_str!("modules.json");

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn test_modules_json_is_valid() {
        let v: Value = serde_json::from_str(MODULES_JSON).expect("modules.json must be valid JSON");
        assert!(v.get("base").is_some());
    }
}
