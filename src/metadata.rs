use crate::common_parser::read_string;
use std::borrow::Cow;
use nom::{
    self,
    bytes::complete as nom_bytes,
    number::complete as nom_number,
    IResult,
};

#[derive(Debug)]
pub struct ArchiveBlockInfo {
    pub(crate) u_size: u32,
    pub(crate) c_size: u32,
    pub(crate) flags: u16,
}

#[derive(Debug)]
pub struct NodeInfo<'a> {
    pub(crate) offset: u64,
    pub(crate) size: u64,
    pub(crate) status: u32,
    pub(crate) name: Cow<'a, str>,
}

#[derive(Debug)]
pub struct Metadata<'a> {
    pub(crate) guid: &'a [u8],
    pub(crate) blocks: Vec<ArchiveBlockInfo>,
    pub(crate) nodes: Vec<NodeInfo<'a>>,
}

impl<'a> Metadata<'a> {
    pub fn parse(input: &'a [u8]) -> IResult<&[u8], Self> {
        let (input, guid) = nom_bytes::take(16usize)(input)?;
        let (input, block_count) = nom_number::be_u32(input)?;
        let (input, blocks) = nom::multi::count(|input| {
            let (input, u_size) = nom_number::be_u32(input)?;
            let (input, c_size) = nom_number::be_u32(input)?;
            let (input, flags) = nom_number::be_u16(input)?;
            let ret = ArchiveBlockInfo {
                u_size,
                c_size,
                flags,
            };
            Ok((input, ret))
        }, block_count as usize)(input)?;
        let (input, node_count) = nom_number::be_u32(input)?;
        let (input, nodes) = nom::multi::count(|input| {
            let (input, offset) = nom_number::be_u64(input)?;
            let (input, size) = nom_number::be_u64(input)?;
            let (input, status) = nom_number::be_u32(input)?;
            let (input, name) = read_string(input, None)?;
            let ret = NodeInfo {
                offset,
                size,
                status,
                name,
            };
            Ok((input, ret))
        }, node_count as usize)(input)?;
        let ret = Self {
            guid,
            blocks,
            nodes,
        };
        Ok((input, ret))
    }
}
