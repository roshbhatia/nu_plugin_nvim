use rmpv::Value;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuickfixItem {
    pub path: Option<String>,
    pub row: Option<u64>,
    pub column: Option<u64>,
    pub end_row: Option<u64>,
    pub end_column: Option<u64>,
    pub text: Option<String>,
    #[serde(rename = "type")]
    pub item_type: Option<String>,
}

impl QuickfixItem {
    /// # Errors
    /// Returns an error when a zero-based position cannot become one-based.
    pub fn to_rpc_value(&self) -> Result<Value, QuickfixError> {
        let mut fields = Vec::new();
        if let Some(path) = &self.path {
            fields.push((Value::from("filename"), Value::from(path.clone())));
        }
        push_one_based(&mut fields, "lnum", self.row)?;
        push_one_based(&mut fields, "col", self.column)?;
        push_one_based(&mut fields, "end_lnum", self.end_row)?;
        push_one_based(&mut fields, "end_col", self.end_column)?;
        if let Some(text) = &self.text {
            fields.push((Value::from("text"), Value::from(text.clone())));
        }
        if let Some(item_type) = &self.item_type {
            fields.push((Value::from("type"), Value::from(item_type.clone())));
        }
        Ok(Value::Map(fields))
    }

    /// # Errors
    /// Returns an error when the RPC value has an invalid quickfix shape.
    pub fn from_rpc_value(value: &Value) -> Result<Self, QuickfixError> {
        let map = value.as_map().ok_or(QuickfixError::ExpectedMap)?;
        Ok(Self {
            path: string_value(map, "filename").or_else(|| string_value(map, "path")),
            row: zero_based(map, "lnum")?,
            column: zero_based(map, "col")?,
            end_row: zero_based(map, "end_lnum")?,
            end_column: zero_based(map, "end_col")?,
            text: string_value(map, "text"),
            item_type: string_value(map, "type"),
        })
    }
}

fn push_one_based(
    fields: &mut Vec<(Value, Value)>,
    name: &'static str,
    value: Option<u64>,
) -> Result<(), QuickfixError> {
    if let Some(value) = value {
        let converted = value
            .checked_add(1)
            .ok_or(QuickfixError::PositionOverflow(name))?;
        fields.push((Value::from(name), Value::from(converted)));
    }
    Ok(())
}

fn map_value<'a>(map: &'a [(Value, Value)], name: &str) -> Option<&'a Value> {
    map.iter()
        .find_map(|(key, value)| (key.as_str() == Some(name)).then_some(value))
}

fn string_value(map: &[(Value, Value)], name: &str) -> Option<String> {
    map_value(map, name)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn zero_based(map: &[(Value, Value)], name: &'static str) -> Result<Option<u64>, QuickfixError> {
    let Some(value) = map_value(map, name) else {
        return Ok(None);
    };
    let one_based = value.as_u64().ok_or(QuickfixError::InvalidPosition(name))?;
    if one_based == 0 {
        return Ok(None);
    }
    Ok(Some(one_based - 1))
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum QuickfixError {
    #[error("quickfix item must be a map")]
    ExpectedMap,
    #[error("quickfix field {0} must be a non-negative integer")]
    InvalidPosition(&'static str),
    #[error("quickfix field {0} is too large to convert to one-based indexing")]
    PositionOverflow(&'static str),
}

#[cfg(test)]
mod tests {
    use super::QuickfixItem;
    use rmpv::Value;

    #[test]
    fn quickfix_positions_convert_to_one_based_values() {
        let item = QuickfixItem {
            path: Some("/tmp/main.rs".into()),
            row: Some(0),
            column: Some(5),
            end_row: Some(1),
            end_column: None,
            text: Some("error".into()),
            item_type: Some("E".into()),
        };
        let rpc = item.to_rpc_value().expect("item should encode");
        let map = rpc.as_map().expect("item should be a map");
        assert_eq!(Some(1), field(map, "lnum").and_then(Value::as_u64));
        assert_eq!(Some(6), field(map, "col").and_then(Value::as_u64));
        assert_eq!(Some(2), field(map, "end_lnum").and_then(Value::as_u64));
    }

    #[test]
    fn quickfix_positions_convert_back_to_zero_based_values() {
        let rpc = Value::Map(vec![
            (Value::from("filename"), Value::from("/tmp/main.rs")),
            (Value::from("lnum"), Value::from(42)),
            (Value::from("col"), Value::from(7)),
        ]);
        let item = QuickfixItem::from_rpc_value(&rpc).expect("item should decode");
        assert_eq!(Some(41), item.row);
        assert_eq!(Some(6), item.column);
    }

    fn field<'a>(map: &'a [(Value, Value)], name: &str) -> Option<&'a Value> {
        map.iter()
            .find_map(|(key, value)| (key.as_str() == Some(name)).then_some(value))
    }
}
