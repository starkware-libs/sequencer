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
pub struct PropellerUnit {
    pub channel: u32,
    pub publisher: Vec<u8>,
    pub root: Vec<u8>,
    pub signature: Vec<u8>,
    pub index: u32,
    pub shard: Vec<u8>,
    pub proof: Vec<u8>,
}

impl<'a> MessageRead<'a> for PropellerUnit {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(8) => msg.channel = r.read_uint32(bytes)?,
                Ok(18) => msg.publisher = r.read_bytes(bytes)?.to_owned(),
                Ok(26) => msg.root = r.read_bytes(bytes)?.to_owned(),
                Ok(34) => msg.signature = r.read_bytes(bytes)?.to_owned(),
                Ok(40) => msg.index = r.read_uint32(bytes)?,
                Ok(50) => msg.shard = r.read_bytes(bytes)?.to_owned(),
                Ok(58) => msg.proof = r.read_bytes(bytes)?.to_owned(),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for PropellerUnit {
    fn get_size(&self) -> usize {
        0
        + if self.channel == 0u32 { 0 } else { 1 + sizeof_varint(*(&self.channel) as u64) }
        + if self.publisher.is_empty() { 0 } else { 1 + sizeof_len((&self.publisher).len()) }
        + if self.root.is_empty() { 0 } else { 1 + sizeof_len((&self.root).len()) }
        + if self.signature.is_empty() { 0 } else { 1 + sizeof_len((&self.signature).len()) }
        + if self.index == 0u32 { 0 } else { 1 + sizeof_varint(*(&self.index) as u64) }
        + if self.shard.is_empty() { 0 } else { 1 + sizeof_len((&self.shard).len()) }
        + if self.proof.is_empty() { 0 } else { 1 + sizeof_len((&self.proof).len()) }
    }

    fn write_message<W: WriterBackend>(&self, w: &mut Writer<W>) -> Result<()> {
        if self.channel != 0u32 { w.write_with_tag(8, |w| w.write_uint32(*&self.channel))?; }
        if !self.publisher.is_empty() { w.write_with_tag(18, |w| w.write_bytes(&**&self.publisher))?; }
        if !self.root.is_empty() { w.write_with_tag(26, |w| w.write_bytes(&**&self.root))?; }
        if !self.signature.is_empty() { w.write_with_tag(34, |w| w.write_bytes(&**&self.signature))?; }
        if self.index != 0u32 { w.write_with_tag(40, |w| w.write_uint32(*&self.index))?; }
        if !self.shard.is_empty() { w.write_with_tag(50, |w| w.write_bytes(&**&self.shard))?; }
        if !self.proof.is_empty() { w.write_with_tag(58, |w| w.write_bytes(&**&self.proof))?; }
        Ok(())
    }
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct PropellerUnitBatch {
    pub batch: Vec<propeller::pb::PropellerUnit>,
}

impl<'a> MessageRead<'a> for PropellerUnitBatch {
    fn from_reader(r: &mut BytesReader, bytes: &'a [u8]) -> Result<Self> {
        let mut msg = Self::default();
        while !r.is_eof() {
            match r.next_tag(bytes) {
                Ok(10) => msg.batch.push(r.read_message::<propeller::pb::PropellerUnit>(bytes)?),
                Ok(t) => { r.read_unknown(bytes, t)?; }
                Err(e) => return Err(e),
            }
        }
        Ok(msg)
    }
}

impl MessageWrite for PropellerUnitBatch {
    fn get_size(&self) -> usize {
        0
        + self.batch.iter().map(|s| 1 + sizeof_len((s).get_size())).sum::<usize>()
    }

    fn write_message<W: WriterBackend>(&self, w: &mut Writer<W>) -> Result<()> {
        for s in &self.batch { w.write_with_tag(10, |w| w.write_message(s))?; }
        Ok(())
    }
}

