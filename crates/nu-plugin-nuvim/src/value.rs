use std::collections::HashSet;
use std::io::Cursor;

use nu_protocol::{LabeledError, Record, Span, Value as NuValue};
use nuvim_protocol::{HandleKind, NvimHandle};
use rmpv::Value as RpcValue;

/// # Errors
/// Returns an error when a known handle or nested value is malformed.
pub fn msgpack_to_nu(value: &RpcValue, server: &str, span: Span) -> Result<NuValue, LabeledError> {
    match value {
        RpcValue::Nil => Ok(NuValue::nothing(span)),
        RpcValue::Boolean(value) => Ok(NuValue::bool(*value, span)),
        RpcValue::Integer(value) => {
            if let Some(value) = value.as_i64() {
                Ok(NuValue::int(value, span))
            } else if let Some(value) = value.as_u64() {
                Ok(tagged_record(
                    [
                        ("type", NuValue::string("msgpack-uint", span)),
                        ("value", NuValue::string(value.to_string(), span)),
                    ],
                    span,
                ))
            } else {
                Err(conversion_error(
                    "MessagePack integer has no signed or unsigned representation",
                    span,
                ))
            }
        }
        RpcValue::F32(value) => Ok(NuValue::float(f64::from(*value), span)),
        RpcValue::F64(value) => Ok(NuValue::float(*value, span)),
        RpcValue::String(value) => value.as_str().map_or_else(
            || {
                Ok(tagged_record(
                    [
                        ("type", NuValue::string("msgpack-string", span)),
                        ("data", NuValue::binary(value.as_bytes().to_vec(), span)),
                    ],
                    span,
                ))
            },
            |value| Ok(NuValue::string(value, span)),
        ),
        RpcValue::Binary(value) => Ok(NuValue::binary(value.clone(), span)),
        RpcValue::Array(values) => values
            .iter()
            .map(|value| msgpack_to_nu(value, server, span))
            .collect::<Result<Vec<_>, _>>()
            .map(|values| NuValue::list(values, span)),
        RpcValue::Map(entries) => map_to_nu(entries, server, span),
        RpcValue::Ext(tag, data) => extension_to_nu(*tag, data, server, span),
    }
}

/// # Errors
/// Returns an error when the Nushell value has no explicit `MessagePack` mapping
/// or contains a Neovim handle from another server.
pub fn nu_to_msgpack(value: &NuValue, expected_server: &str) -> Result<RpcValue, LabeledError> {
    let span = value.span();
    match value {
        NuValue::Nothing { .. } => Ok(RpcValue::Nil),
        NuValue::Bool { val, .. } => Ok(RpcValue::Boolean(*val)),
        NuValue::Int { val, .. } => Ok(RpcValue::from(*val)),
        NuValue::Float { val, .. } => Ok(RpcValue::F64(*val)),
        NuValue::String { val, .. } | NuValue::Glob { val, .. } => Ok(RpcValue::from(val.clone())),
        NuValue::Binary { val, .. } => Ok(RpcValue::Binary(val.to_vec())),
        NuValue::List { vals, .. } => vals
            .iter()
            .map(|value| nu_to_msgpack(value, expected_server))
            .collect::<Result<Vec<_>, _>>()
            .map(RpcValue::Array),
        NuValue::Record { val, .. } => record_to_msgpack(val, span, expected_server),
        other => Err(conversion_error(
            format!(
                "Nushell {} values cannot be sent through Neovim RPC",
                other.get_type()
            ),
            span,
        )),
    }
}

fn map_to_nu(
    entries: &[(RpcValue, RpcValue)],
    server: &str,
    span: Span,
) -> Result<NuValue, LabeledError> {
    let mut names = HashSet::new();
    let direct = entries.iter().all(|(key, _)| {
        key.as_str()
            .is_some_and(|name| names.insert(name.to_owned()))
    });
    if direct {
        let record = entries
            .iter()
            .map(|(key, value)| {
                Ok((
                    key.as_str().expect("map keys were checked").to_owned(),
                    msgpack_to_nu(value, server, span)?,
                ))
            })
            .collect::<Result<Record, LabeledError>>()?;
        return Ok(NuValue::record(record, span));
    }

    let converted = entries
        .iter()
        .map(|(key, value)| {
            Ok(tagged_record(
                [
                    ("key", msgpack_to_nu(key, server, span)?),
                    ("value", msgpack_to_nu(value, server, span)?),
                ],
                span,
            ))
        })
        .collect::<Result<Vec<_>, LabeledError>>()?;
    Ok(tagged_record(
        [
            ("type", NuValue::string("msgpack-map", span)),
            ("entries", NuValue::list(converted, span)),
        ],
        span,
    ))
}

fn extension_to_nu(
    tag: i8,
    data: &[u8],
    server: &str,
    span: Span,
) -> Result<NuValue, LabeledError> {
    if let Some(kind) = HandleKind::from_extension_tag(tag) {
        let handle = NvimHandle::from_rpc_value(&RpcValue::Ext(tag, data.to_vec()))
            .map_err(|error| conversion_error(error.to_string(), span))?;
        let id = i64::try_from(handle.id).map_err(|_| {
            conversion_error("Neovim handle ID exceeds Nushell integer range", span)
        })?;
        return Ok(tagged_record(
            [
                ("type", NuValue::string("nvim-handle", span)),
                ("kind", NuValue::string(kind.as_str(), span)),
                ("id", NuValue::int(id, span)),
                ("server", NuValue::string(server, span)),
            ],
            span,
        ));
    }
    Ok(tagged_record(
        [
            ("type", NuValue::string("msgpack-ext", span)),
            ("tag", NuValue::int(i64::from(tag), span)),
            ("data", NuValue::binary(data.to_vec(), span)),
        ],
        span,
    ))
}

fn record_to_msgpack(
    record: &Record,
    span: Span,
    expected_server: &str,
) -> Result<RpcValue, LabeledError> {
    match string_field(record, "type") {
        Some("nvim-handle") => handle_record_to_msgpack(record, span, expected_server),
        Some("msgpack-ext") => extension_record_to_msgpack(record, span),
        Some("msgpack-map") => tagged_map_to_msgpack(record, span, expected_server),
        Some("msgpack-uint") => unsigned_record_to_msgpack(record, span),
        Some("msgpack-string") => invalid_string_record_to_msgpack(record, span),
        _ => record
            .iter()
            .map(|(name, value)| {
                Ok((
                    RpcValue::from(name.clone()),
                    nu_to_msgpack(value, expected_server)?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(RpcValue::Map),
    }
}

fn handle_record_to_msgpack(
    record: &Record,
    span: Span,
    expected_server: &str,
) -> Result<RpcValue, LabeledError> {
    let handle_server = string_field(record, "server")
        .ok_or_else(|| conversion_error("Neovim handle record has no server", span))?;
    if handle_server != expected_server {
        return Err(conversion_error(
            format!(
                "Neovim handle belongs to server {handle_server}, not target server {expected_server}"
            ),
            span,
        ));
    }
    let kind = match string_field(record, "kind") {
        Some("buffer") => HandleKind::Buffer,
        Some("window") => HandleKind::Window,
        Some("tab" | "tabpage") => HandleKind::TabPage,
        Some(kind) => {
            return Err(conversion_error(
                format!("unknown Neovim handle kind {kind}"),
                span,
            ));
        }
        None => return Err(conversion_error("Neovim handle record has no kind", span)),
    };
    let id = integer_field(record, "id")?;
    let id = u64::try_from(id)
        .map_err(|_| conversion_error("Neovim handle ID must be non-negative", span))?;
    NvimHandle::new(kind, id)
        .to_rpc_value()
        .map_err(|error| conversion_error(error.to_string(), span))
}

fn extension_record_to_msgpack(record: &Record, span: Span) -> Result<RpcValue, LabeledError> {
    let tag = integer_field(record, "tag")?;
    let tag = i8::try_from(tag)
        .map_err(|_| conversion_error("MessagePack extension tag must fit in i8", span))?;
    let data = binary_field(record, "data")?;
    Ok(RpcValue::Ext(tag, data.to_vec()))
}

fn tagged_map_to_msgpack(
    record: &Record,
    span: Span,
    expected_server: &str,
) -> Result<RpcValue, LabeledError> {
    let entries = field(record, "entries")
        .ok_or_else(|| conversion_error("tagged MessagePack map has no entries", span))?
        .as_list()
        .map_err(|_| conversion_error("tagged MessagePack map entries must be a list", span))?;
    entries
        .iter()
        .map(|entry| {
            let entry = entry.as_record().map_err(|_| {
                conversion_error(
                    "tagged MessagePack map entry must be a record",
                    entry.span(),
                )
            })?;
            let key = field(entry, "key")
                .ok_or_else(|| conversion_error("tagged map entry has no key", span))?;
            let value = field(entry, "value")
                .ok_or_else(|| conversion_error("tagged map entry has no value", span))?;
            Ok((
                nu_to_msgpack(key, expected_server)?,
                nu_to_msgpack(value, expected_server)?,
            ))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(RpcValue::Map)
}

fn unsigned_record_to_msgpack(record: &Record, span: Span) -> Result<RpcValue, LabeledError> {
    let raw = string_field(record, "value")
        .ok_or_else(|| conversion_error("tagged unsigned integer has no value", span))?;
    raw.parse::<u64>().map(RpcValue::from).map_err(|error| {
        conversion_error(format!("invalid tagged unsigned integer: {error}"), span)
    })
}

fn invalid_string_record_to_msgpack(record: &Record, span: Span) -> Result<RpcValue, LabeledError> {
    let data = binary_field(record, "data")?;
    let mut encoded = Vec::new();
    rmpv::encode::write_value(&mut encoded, &RpcValue::Binary(data.to_vec())).map_err(|error| {
        conversion_error(
            format!("could not encode MessagePack string: {error}"),
            span,
        )
    })?;
    encoded[0] = match encoded[0] {
        0xc4 => 0xd9,
        0xc5 => 0xda,
        0xc6 => 0xdb,
        _ => {
            return Err(conversion_error(
                "unsupported MessagePack string length",
                span,
            ));
        }
    };
    rmpv::decode::read_value(&mut Cursor::new(encoded)).map_err(|error| {
        conversion_error(
            format!("could not decode MessagePack string: {error}"),
            span,
        )
    })
}

fn field<'a>(record: &'a Record, name: &str) -> Option<&'a NuValue> {
    record
        .iter()
        .find_map(|(key, value)| (key == name).then_some(value))
}

fn string_field<'a>(record: &'a Record, name: &str) -> Option<&'a str> {
    field(record, name).and_then(|value| value.as_str().ok())
}

fn integer_field(record: &Record, name: &str) -> Result<i64, LabeledError> {
    let value = field(record, name)
        .ok_or_else(|| conversion_error(format!("record has no {name} field"), Span::unknown()))?;
    value.as_int().map_err(|_| {
        conversion_error(
            format!("record field {name} must be an integer"),
            value.span(),
        )
    })
}

fn binary_field<'a>(record: &'a Record, name: &str) -> Result<&'a [u8], LabeledError> {
    let value = field(record, name)
        .ok_or_else(|| conversion_error(format!("record has no {name} field"), Span::unknown()))?;
    value
        .as_binary()
        .map_err(|_| conversion_error(format!("record field {name} must be binary"), value.span()))
}

fn tagged_record<const N: usize>(fields: [(&str, NuValue); N], span: Span) -> NuValue {
    NuValue::record(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
        span,
    )
}

fn conversion_error(message: impl Into<String>, span: Span) -> LabeledError {
    LabeledError::new(message)
        .with_code("nuvim::value_conversion")
        .with_label("cannot convert this value", span)
}

#[cfg(test)]
mod tests {
    use nu_protocol::{Record, Span, Value as NuValue};
    use nuvim_protocol::{HandleKind, NvimHandle};
    use rmpv::Value as RpcValue;

    use super::{msgpack_to_nu, nu_to_msgpack};

    const SERVER: &str = "/tmp/nvim.sock";

    #[test]
    fn direct_values_round_trip() {
        let values = [
            RpcValue::Nil,
            RpcValue::Boolean(true),
            RpcValue::from(-12),
            RpcValue::F64(1.25),
            RpcValue::from("hello"),
            RpcValue::Binary(vec![0, 1, 255]),
            RpcValue::Array(vec![RpcValue::from(1), RpcValue::from("two")]),
            RpcValue::Map(vec![(RpcValue::from("key"), RpcValue::from("value"))]),
        ];
        for value in values {
            let nu = msgpack_to_nu(&value, SERVER, Span::test_data())
                .expect("MessagePack should convert");
            assert_eq!(
                value,
                nu_to_msgpack(&nu, SERVER).expect("Nushell should convert")
            );
        }
    }

    #[test]
    fn non_string_map_keys_use_tagged_representation() {
        let value = RpcValue::Map(vec![(RpcValue::from(4), RpcValue::from("value"))]);
        let nu = msgpack_to_nu(&value, SERVER, Span::test_data()).expect("map should convert");
        let record = nu.as_record().expect("tagged map should be a record");
        assert_eq!(Some("msgpack-map"), string_field(record, "type"));
        assert_eq!(
            value,
            nu_to_msgpack(&nu, SERVER).expect("tagged map should round trip")
        );
    }

    #[test]
    fn unknown_extension_uses_tagged_representation() {
        let value = RpcValue::Ext(9, vec![1, 2, 3]);
        let nu =
            msgpack_to_nu(&value, SERVER, Span::test_data()).expect("extension should convert");
        let record = nu.as_record().expect("tagged extension should be a record");
        assert_eq!(Some("msgpack-ext"), string_field(record, "type"));
        assert_eq!(
            value,
            nu_to_msgpack(&nu, SERVER).expect("extension should round trip")
        );
    }

    #[test]
    fn unsupported_nushell_values_return_labeled_error() {
        let value = NuValue::filesize(42, Span::test_data());
        let error =
            nu_to_msgpack(&value, SERVER).expect_err("filesize should not convert implicitly");
        assert_eq!(Some("nuvim::value_conversion"), error.code.as_deref());
        assert_eq!(1, error.labels.len());
    }

    #[test]
    fn nested_handle_from_another_server_is_rejected() {
        let handle = NvimHandle::new(HandleKind::Buffer, 1)
            .to_rpc_value()
            .expect("handle should encode");
        let handle = msgpack_to_nu(&handle, "/tmp/other.sock", Span::test_data())
            .expect("handle should convert");
        let nested = NuValue::record(
            Record::from_iter([(
                "payload".into(),
                NuValue::list(vec![handle], Span::test_data()),
            )]),
            Span::test_data(),
        );

        let error =
            nu_to_msgpack(&nested, SERVER).expect_err("a cross-server handle should not convert");

        assert!(
            error
                .to_string()
                .contains("belongs to server /tmp/other.sock")
        );
        assert!(error.to_string().contains("target server /tmp/nvim.sock"));
    }

    fn string_field<'a>(record: &'a Record, name: &str) -> Option<&'a str> {
        record
            .iter()
            .find_map(|(key, value)| (key == name).then_some(value))
            .and_then(|value| value.as_str().ok())
    }
}
