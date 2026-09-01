use std::ffi::OsString;
use std::io::Write;
use std::process::{Command, Stdio};

use nvim_oxi::{Dictionary, Function, Object};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[nvim_oxi::plugin]
fn nvim_nu() -> Dictionary {
    Dictionary::from_iter([
        (
            "eval",
            Object::from(Function::<String, Object>::from_fn(eval)),
        ),
        (
            "filter",
            Object::from(Function::<(String, Object), Object>::from_fn(filter)),
        ),
        (
            "call",
            Object::from(Function::<(String, Object), Object>::from_fn(call)),
        ),
    ])
}

fn eval(code: String) -> Result<Object, NuError> {
    let code = code.into_boxed_str();
    run_nu("eval", &format!("({code}) | to json --raw"), None)
}

fn filter((code, input): (String, Object)) -> Result<Object, NuError> {
    let input = object_to_json(input)?;
    run_nu(
        "filter",
        &format!("open --raw /dev/stdin | from json | {code} | to json --raw"),
        Some(&input),
    )
}

fn call((handler, input): (String, Object)) -> Result<Object, NuError> {
    if handler.trim().is_empty() {
        return Err(NuError::InvalidHandler);
    }
    let input = object_to_json(input)?;
    run_nu(
        &format!("call {handler}"),
        &format!("open --raw /dev/stdin | from json | {handler} | to json --raw"),
        Some(&input),
    )
}

fn run_nu(context: &str, source: &str, input: Option<&str>) -> Result<Object, NuError> {
    let mut child = Command::new(nu_program())
        .args(["--no-config-file", "-c", source])
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| NuError::Spawn {
            context: context.into(),
            source,
        })?;

    if let Some(input) = input {
        child
            .stdin
            .take()
            .ok_or_else(|| NuError::Stdin(context.into()))?
            .write_all(input.as_bytes())
            .map_err(|source| NuError::Write {
                context: context.into(),
                source,
            })?;
    }

    let output = child.wait_with_output().map_err(|source| NuError::Wait {
        context: context.into(),
        source,
    })?;
    if !output.status.success() {
        return Err(NuError::Exit {
            context: context.into(),
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    let stdout = String::from_utf8(output.stdout).map_err(|source| NuError::Utf8 {
        context: context.into(),
        source,
    })?;
    if stdout.trim().is_empty() {
        return Ok(Object::nil());
    }
    let json: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|source| NuError::JsonOutput {
            context: context.into(),
            output: stdout,
            source,
        })?;
    json_to_object(&json)
}

fn nu_program() -> OsString {
    std::env::var_os("NUVIM_NU_BIN")
        .unwrap_or_else(|| option_env!("NUVIM_NU_BIN").unwrap_or("nu").into())
}

fn object_to_json(object: Object) -> Result<String, NuError> {
    let json = serde_json::Value::deserialize(nvim_oxi::serde::Deserializer::new(object))
        .map_err(NuError::NvimDeserialize)?;
    serde_json::to_string(&json).map_err(NuError::JsonInput)
}

fn json_to_object(json: &serde_json::Value) -> Result<Object, NuError> {
    json.serialize(nvim_oxi::serde::Serializer::new())
        .map_err(NuError::NvimSerialize)
}

#[derive(Debug, Error)]
enum NuError {
    #[error("nu.{context} could not start Nushell: {source}")]
    Spawn {
        context: String,
        source: std::io::Error,
    },
    #[error("nu.{0} could not access Nushell stdin")]
    Stdin(String),
    #[error("nu.{context} could not write Nushell input: {source}")]
    Write {
        context: String,
        source: std::io::Error,
    },
    #[error("nu.{context} could not wait for Nushell: {source}")]
    Wait {
        context: String,
        source: std::io::Error,
    },
    #[error("nu.{context} failed with {status}: {stderr}")]
    Exit {
        context: String,
        status: String,
        stderr: String,
    },
    #[error("nu.{context} returned non-UTF-8 output: {source}")]
    Utf8 {
        context: String,
        source: std::string::FromUtf8Error,
    },
    #[error("nu.{context} returned malformed JSON: {source}; output: {output}")]
    JsonOutput {
        context: String,
        output: String,
        source: serde_json::Error,
    },
    #[error("could not serialize Neovim input as JSON: {0}")]
    JsonInput(serde_json::Error),
    #[error("could not convert Neovim input: {0}")]
    NvimDeserialize(nvim_oxi::serde::DeserializeError),
    #[error("could not convert Nushell output: {0}")]
    NvimSerialize(nvim_oxi::serde::SerializeError),
    #[error("nu.call needs a non-empty handler name")]
    InvalidHandler,
}

#[cfg(test)]
mod tests {
    use super::run_nu;

    #[test]
    fn eval_returns_structured_json() {
        let value = run_nu("test", "{answer: 42} | to json --raw", None)
            .expect("Nushell should return an object");
        assert_eq!("dictionary", value.kind().as_static());
    }
}
