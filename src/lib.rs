mod asset;
mod common_parser;
mod compression;
mod metadata;
mod util;

use crate::common_parser::read_string;
use std::borrow::Cow;
use nom::{
    number::complete as nom_number,
    IResult,
};

pub use asset::Asset;
pub use compression::CompressedBlock;
pub use metadata::Metadata;

pub struct UnityFsMeta<'a> {
    signature: Cow<'a, str>,
    format_version: u32,
    unity_version: Cow<'a, str>,
    generator_version: Cow<'a, str>,
    metadata: Metadata,
    storage: compression::CompressedBlockStorage<'a>,
}

impl<'a> UnityFsMeta<'a> {
    pub fn signature(&self) -> &str {
        &self.signature
    }

    pub fn format_version(&self) -> u32 {
        self.format_version
    }

    pub fn unity_version(&self) -> &str {
        &self.unity_version
    }

    pub fn generator_version(&self) -> &str {
        &self.generator_version
    }
}

impl<'a> UnityFsMeta<'a> {
    pub fn parse(input: &'a [u8]) -> IResult<&[u8], Self> {
        let (input, signature) = read_string(input, None)?;
        let (input, format_version) = nom_number::be_u32(input)?;
        let (input, unity_version) = read_string(input, None)?;
        let (input, generator_version) = read_string(input, None)?;
        let (input, _file_size) = nom_number::be_u64(input)?;

        let (input, c_block_size) = nom_number::be_u32(input)?;
        let (input, u_block_size) = nom_number::be_u32(input)?;
        let (input, flags) = nom_number::be_u32(input)?;
        let (input, raw_metadata) = if flags & 0x80 == 0x80 {
            input.split_at(input.len() - c_block_size as usize)
        } else {
            let (a, b) = input.split_at(c_block_size as usize);
            (b, a)
        };
        let metadata = CompressedBlock::from_slice(u_block_size, flags & 0x3f, raw_metadata);
        let metadata = metadata.decompress();
        let metadata = match Metadata::parse(&metadata) {
            Ok((_, metadata)) => metadata,
            Err(nom::Err::Error((_, e))) => return Err(nom::Err::Error((input, e))),
            Err(nom::Err::Failure((_, e))) => return Err(nom::Err::Failure((input, e))),
            Err(nom::Err::Incomplete(n)) => return Err(nom::Err::Incomplete(n)),
        };

        let mut left = input;
        let blocks = metadata.blocks.iter().map(|block| {
            let (data, remainder) = left.split_at(block.c_size as usize);
            left = remainder;
            CompressedBlock::from_slice(block.u_size, (block.flags & 0x3f) as u32, data)
        }).collect();
        let storage = compression::CompressedBlockStorage::from_blocks(blocks);

        Ok((left, UnityFsMeta {
            signature,
            format_version,
            unity_version,
            generator_version,
            metadata,
            storage,
        }))
    }

    pub fn read_unityfs(&self) -> UnityFs<'_> {
        let assets = self.metadata.nodes.iter().map(|node| {
            let block = self.storage.read_range(node.offset..(node.offset + node.size));
            Asset::parse(node.name.clone(), block, node.offset).map(|(_, asset)| asset)
        }).collect::<Result<_, _>>().unwrap();
        UnityFs {
            guid: self.metadata.guid,
            assets,
        }
    }
}

#[derive(Debug)]
pub struct UnityFs<'b> {
    guid: [u8; 16],
    assets: Vec<Asset<'b>>,
}

impl<'b> UnityFs<'b> {
    pub fn guid(&self) -> [u8; 16] {
        self.guid
    }

    pub fn assets(&self) -> &[Asset<'b>] {
        &self.assets
    }
}
