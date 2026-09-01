use std::io::Cursor;

use rmpv::Value;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandleKind {
    Buffer,
    Window,
    TabPage,
}

impl HandleKind {
    #[must_use]
    pub const fn extension_tag(self) -> i8 {
        match self {
            Self::Buffer => 0,
            Self::Window => 1,
            Self::TabPage => 2,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Buffer => "buffer",
            Self::Window => "window",
            Self::TabPage => "tab",
        }
    }

    #[must_use]
    pub const fn from_extension_tag(tag: i8) -> Option<Self> {
        match tag {
            0 => Some(Self::Buffer),
            1 => Some(Self::Window),
            2 => Some(Self::TabPage),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NvimHandle {
    pub kind: HandleKind,
    pub id: u64,
}

impl NvimHandle {
    #[must_use]
    pub const fn new(kind: HandleKind, id: u64) -> Self {
        Self { kind, id }
    }

    /// # Errors
    /// Returns an error when `MessagePack` cannot encode the handle ID.
    pub fn to_rpc_value(self) -> Result<Value, HandleError> {
        let mut data = Vec::new();
        rmpv::encode::write_value(&mut data, &Value::from(self.id))?;
        Ok(Value::Ext(self.kind.extension_tag(), data))
    }

    /// # Errors
    /// Returns an error when the value is not a valid Neovim handle extension.
    pub fn from_rpc_value(value: &Value) -> Result<Self, HandleError> {
        let Value::Ext(tag, data) = value else {
            return Err(HandleError::NotHandle);
        };
        let kind = HandleKind::from_extension_tag(*tag).ok_or(HandleError::UnknownTag(*tag))?;
        let id_value = rmpv::decode::read_value(&mut Cursor::new(data))?;
        let id = id_value.as_u64().ok_or(HandleError::InvalidId)?;
        Ok(Self { kind, id })
    }
}

#[derive(Debug, Error)]
pub enum HandleError {
    #[error("value is not a Neovim handle")]
    NotHandle,
    #[error("unknown Neovim handle extension tag {0}")]
    UnknownTag(i8),
    #[error("Neovim handle payload is not an unsigned integer")]
    InvalidId,
    #[error("could not encode Neovim handle: {0}")]
    Encode(#[from] rmpv::encode::Error),
    #[error("could not decode Neovim handle: {0}")]
    Decode(#[from] rmpv::decode::Error),
}

#[cfg(test)]
mod tests {
    use super::{HandleKind, NvimHandle};

    #[test]
    fn handle_round_trip_uses_msgpack_integer_payload() {
        let handle = NvimHandle::new(HandleKind::Buffer, 513);
        let encoded = handle.to_rpc_value().expect("handle should encode");
        assert_eq!(
            handle,
            NvimHandle::from_rpc_value(&encoded).expect("handle should decode")
        );
    }
}
