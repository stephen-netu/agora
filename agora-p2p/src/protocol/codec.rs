use crate::error::Error;
use crate::protocol::AmpMessage;
use ciborium::de;
use ciborium::ser;
use std::io::Cursor;

pub fn encode(message: &AmpMessage) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    let mut writer = Cursor::new(&mut bytes);
    ser::into_writer(message, &mut writer).map_err(|e| Error::Protocol(e.to_string()))?;
    Ok(bytes)
}

pub fn decode(bytes: &[u8]) -> Result<AmpMessage, Error> {
    let mut reader = Cursor::new(bytes);
    de::from_reader(&mut reader).map_err(|e| Error::Protocol(e.to_string()))
}
