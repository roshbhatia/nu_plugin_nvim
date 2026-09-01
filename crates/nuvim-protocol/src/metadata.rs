use std::collections::BTreeMap;

use rmpv::Value;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiParameter {
    pub type_name: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiFunction {
    pub name: String,
    pub parameters: Vec<ApiParameter>,
    pub return_type: String,
    pub method: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ApiMetadata {
    functions: BTreeMap<String, ApiFunction>,
}

impl ApiMetadata {
    /// # Errors
    /// Returns an error when `nvim_get_api_info` metadata has an unexpected shape.
    pub fn from_api_info(value: &Value) -> Result<Self, MetadataError> {
        let items = value.as_array().ok_or(MetadataError::ApiInfoShape)?;
        let metadata = items
            .get(1)
            .and_then(Value::as_map)
            .ok_or(MetadataError::ApiInfoShape)?;
        let functions = map_get(metadata, "functions")
            .and_then(Value::as_array)
            .ok_or(MetadataError::MissingFunctions)?;
        let mut parsed = BTreeMap::new();
        for function in functions {
            let map = function.as_map().ok_or(MetadataError::FunctionShape)?;
            let name = string_field(map, "name")?;
            let return_type = string_field(map, "return_type")?;
            let method = map_get(map, "method")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let raw_parameters = map_get(map, "parameters")
                .and_then(Value::as_array)
                .ok_or(MetadataError::FunctionShape)?;
            let mut parameters = Vec::with_capacity(raw_parameters.len());
            for parameter in raw_parameters {
                let pair = parameter.as_array().ok_or(MetadataError::ParameterShape)?;
                parameters.push(ApiParameter {
                    type_name: pair
                        .first()
                        .and_then(Value::as_str)
                        .ok_or(MetadataError::ParameterShape)?
                        .into(),
                    name: pair
                        .get(1)
                        .and_then(Value::as_str)
                        .ok_or(MetadataError::ParameterShape)?
                        .into(),
                });
            }
            parsed.insert(
                name.clone(),
                ApiFunction {
                    name,
                    parameters,
                    return_type,
                    method,
                },
            );
        }
        Ok(Self { functions: parsed })
    }

    #[must_use]
    pub fn function(&self, name: &str) -> Option<&ApiFunction> {
        self.functions.get(name)
    }

    pub fn function_names(&self) -> impl Iterator<Item = &str> {
        self.functions.keys().map(String::as_str)
    }
}

fn map_get<'a>(map: &'a [(Value, Value)], key: &str) -> Option<&'a Value> {
    map.iter()
        .find_map(|(candidate, value)| (candidate.as_str() == Some(key)).then_some(value))
}

fn string_field(map: &[(Value, Value)], key: &'static str) -> Result<String, MetadataError> {
    map_get(map, key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(MetadataError::MissingField(key))
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum MetadataError {
    #[error("nvim_get_api_info returned an unexpected value")]
    ApiInfoShape,
    #[error("Neovim API metadata has no functions array")]
    MissingFunctions,
    #[error("Neovim API function metadata has an unexpected shape")]
    FunctionShape,
    #[error("Neovim API parameter metadata has an unexpected shape")]
    ParameterShape,
    #[error("Neovim API metadata is missing {0}")]
    MissingField(&'static str),
}
