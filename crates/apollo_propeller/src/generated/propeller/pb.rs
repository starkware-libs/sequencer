// Automatically generated rust module for 'propeller.proto' file

#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(unused_imports)]
#![allow(unknown_lints)]
#![allow(clippy::all)]
#![cfg_attr(rustfmt, rustfmt_skip)]


use quick_protobuf::{MessageInfo, MessageRead, MessageWrite, BytesReader, Writer, WriterBackend, Result};
use quick_protobuf::sizeofs::*;
use super::super::*;

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct PropellerMessage {
    pub root: Vec<u8>,
    pub publisher: Vec<u8>,
    pub signature: Vec<u8>,
    pub index: u32,
    pub shard: Vec<u8>,
    pub proof: Vec<u8>,
}

impl<'a> MessageRead<'a> for PropellerMessage {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.root = r.read_bytes(bytes)?.to_owned(),
                Ok(18) => msg.publisher = r.read_bytes(bytes)?.to_owned(),
                Ok(26) => msg.signature = r.read_bytes(bytes)?.to_owned(),
                Ok(32) => msg.index = r.read_uint32(bytes)?,
                Ok(42) => msg.shard = r.read_bytes(bytes)?.to_owned(),
                Ok(50) => msg.proof = r.read_bytes(bytes)?.to_owned(),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for PropellerMessage {
    fn get_size(&self) -> usize {
        0
        + if self.root.is_empty() { 0 } else { 1 + sizeof_len((&self.root).len()) }
        + if self.publisher.is_empty() { 0 } else { 1 + sizeof_len((&self.publisher).len()) }
        + if self.signature.is_empty() { 0 } else { 1 + sizeof_len((&self.signature).len()) }
        + if self.index == 0u32 { 0 } else { 1 + sizeof_varint(*(&self.index) as u64) }
        + if self.shard.is_empty() { 0 } else { 1 + sizeof_len((&self.shard).len()) }
        + if self.proof.is_empty() { 0 } else { 1 + sizeof_len((&self.proof).len()) }
    }

    fn write_message<W: WriterBackend>(&self, w: &mut Writer<W>) -> Result<()> {
        if !self.root.is_empty() { w.write_with_tag(10, |w| w.write_bytes(&**&self.root))?; }
        if !self.publisher.is_empty() { w.write_with_tag(18, |w| w.write_bytes(&**&self.publisher))?; }
        if !self.signature.is_empty() { w.write_with_tag(26, |w| w.write_bytes(&**&self.signature))?; }
        if self.index != 0u32 { w.write_with_tag(32, |w| w.write_uint32(*&self.index))?; }
        if !self.shard.is_empty() { w.write_with_tag(42, |w| w.write_bytes(&**&self.shard))?; }
        if !self.proof.is_empty() { w.write_with_tag(50, |w| w.write_bytes(&**&self.proof))?; }
        Ok(())
    }
}

