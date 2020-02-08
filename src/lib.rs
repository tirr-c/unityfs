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
    file_size: u64,
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

    pub fn file_size(&self) -> u64 {
        self.file_size
    }
}

pub fn read_unityfs_meta(input: &[u8]) -> IResult<&[u8], (UnityFsMeta, CompressedBlock)> {
    let (input, signature) = read_string(input, None)?;
    let (input, format_version) = nom_number::be_u32(input)?;
    let (input, unity_version) = read_string(input, None)?;
    let (input, generator_version) = read_string(input, None)?;
    let (input, file_size) = nom_number::be_u64(input)?;
    let unityfs = UnityFsMeta {
        signature,
        format_version,
        unity_version,
        generator_version,
        file_size,
    };

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

    Ok((input, (unityfs, metadata)))
}

#[derive(Debug)]
pub struct UnityFs<'m, 'b> {
    guid: &'m [u8],
    assets: Vec<Asset<'m, 'b>>,
}

impl<'m, 'b> UnityFs<'m, 'b> {
    pub fn guid(&self) -> &'m [u8] {
        self.guid
    }

    pub fn assets(&self) -> &[Asset<'m, 'b>] {
        &self.assets
    }
}

pub fn read_blocks<'b>(mut left: &'b [u8], metadata: &Metadata<'_>) -> compression::CompressedBlockStorage<'b> {
    let blocks = metadata.blocks.iter().map(|block| {
        let (data, remainder) = left.split_at(block.c_size as usize);
        left = remainder;
        CompressedBlock::from_slice(block.u_size, (block.flags & 0x3f) as u32, data)
    }).collect();
    compression::CompressedBlockStorage::from_blocks(blocks)
}

pub fn read_unityfs<'m, 'b>(metadata: Metadata<'m>, block_storage: &'b compression::CompressedBlockStorage<'b>) -> UnityFs<'m, 'b> {
    let assets = metadata.nodes.into_iter().map(|node| {
        let block = block_storage.read_range(node.offset..(node.offset + node.size));
        Asset::parse(node.name, block, node.offset).map(|(_, asset)| asset)
    }).collect::<Result<_, _>>().unwrap();
    UnityFs {
        guid: metadata.guid,
        assets,
    }
}
