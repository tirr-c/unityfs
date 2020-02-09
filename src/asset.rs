mod type_tree;

use crate::common_parser::read_string;
use crate::util::align;
use std::borrow::Cow;

use nom::{
    number::{
        complete as nom_number,
        Endianness,
    },
    u64,
    u32,
    i32,
    i16,
    IResult,
};

use type_tree::TypeMetadata;
pub use type_tree::Data;

#[derive(Debug)]
pub struct Asset<'b> {
    name: String,
    metadata_size: u32,
    file_size: u32,
    format: u32,
    data_offset: u32,
    tree: TypeMetadata<'b>,
    objects: Vec<Object<'b>>,
    adds: Vec<(u64, i32)>,
    refs: Vec<AssetRef<'b>>,
}

#[derive(Debug)]
pub struct Object<'b> {
    pub path_id: u64,
    pub type_id: i32,
    pub class_id: i32,
    pub is_destroyed: bool,
    pub data: Data<'b>,
}

#[derive(Debug)]
pub struct AssetRef<'b> {
    asset_path: Cow<'b, str>,
    guid: &'b [u8],
    ty: u32,
    file_path: Cow<'b, str>,
}

impl<'b> Asset<'b> {
    pub(crate) fn parse(name: String, input: &'b [u8], offset: u64) -> IResult<&[u8], Self> {
        let base = input;
        let (input, metadata_size) = nom_number::be_u32(input)?;
        let (input, file_size) = nom_number::be_u32(input)?;
        let (input, format) = nom_number::be_u32(input)?;
        let (input, data_offset) = nom_number::be_u32(input)?;
        let (input, endianness) = if format >= 9 {
            let (input, endianness) = nom_number::be_u32(input)?;
            let endianness = if endianness == 0 { Endianness::Little } else { Endianness::Big };
            (input, endianness)
        } else {
            (input, Endianness::Big)
        };
        let (input, tree) = TypeMetadata::parse(input, endianness, format)?;
        let (input, long_object_ids) = if (7..=13).contains(&format) {
            let (input, long_object_ids) = u32!(input, endianness)?;
            (input, long_object_ids != 0)
        } else {
            (input, false)
        };
        let (mut input_out, num_objects) = u32!(input, endianness)?;
        let objects = (0..num_objects).map(|_| {
            let input = if format >= 14 {
                align(offset as usize, base, input_out)
            } else {
                input_out
            };
            let (input, path_id) = if format >= 14 || long_object_ids {
                u64!(input, endianness)?
            } else {
                let (input, id) = u32!(input, endianness)?;
                (input, id.into())
            };
            let (input, object_data_offset) = u32!(input, endianness)?;
            let (input, size) = u32!(input, endianness)?;
            let start = data_offset + object_data_offset;
            let end = start + size;

            let (input, type_id, class_id) = if format < 17 {
                let (input, type_id) = i32!(input, endianness)?;
                let (input, class_id) = i16!(input, endianness)?;
                (input, type_id, class_id.into())
            } else {
                let (input, type_id) = u32!(input, endianness)?;
                let class_id = tree.class_id_from_idx(type_id as usize);
                (input, class_id, class_id)
            };
            let data = {
                let type_tree = tree.type_tree_from_id(type_id, class_id).unwrap();
                let (_, data) = type_tree.read(&base[start as usize..end as usize], endianness, 0)?;
                data
            };

            let (input, is_destroyed) = if format <= 10 {
                let val = input[0] != 0;
                (&input[1..], val)
            } else {
                (input, false)
            };
            let input = if (11..=16).contains(&format) {
                &input[2..]
            } else {
                input
            };
            let input = if (15..=16).contains(&format) {
                &input[1..]
            } else {
                input
            };
            input_out = input;
            Ok(Object {
                path_id,
                type_id,
                class_id,
                is_destroyed,
                data,
            })
        }).collect::<Result<Vec<_>, _>>()?;

        let (input, adds) = if format >= 11 {
            let (mut input_out, add_count) = u32!(input_out, endianness)?;
            let adds = (0..add_count).map(|_| {
                let input = if format >= 14 {
                    align(offset as usize, base, input_out)
                } else {
                    input_out
                };
                let (input, add_id) = if format >= 14 {
                    u64!(input, endianness)?
                } else {
                    let (input, id) = u32!(input, endianness)?;
                    (input, id.into())
                };
                let (input, value) = i32!(input, endianness)?;
                input_out = input;
                Ok((add_id, value))
            }).collect::<Result<Vec<(u64, i32)>, _>>()?;
            (input_out, adds)
        } else {
            (input_out, Vec::new())
        };

        let (input, refs) = if format >= 16 {
            let (mut input_out, refs_count) = u32!(input, endianness)?;
            let refs = (0..refs_count).map(|_| {
                let (input, asset_path) = read_string(input_out, None)?;
                let (guid, input) = input.split_at(0x10);
                let (input, ty) = u32!(input, endianness)?;
                let (input, file_path) = read_string(input, None)?;
                input_out = input;
                Ok(AssetRef {
                    asset_path,
                    guid,
                    ty,
                    file_path,
                })
            }).collect::<Result<Vec<_>, _>>()?;
            (input_out, refs)
        } else {
            (input, Vec::new())
        };
        let (input, _) = read_string(input, None)?;

        let asset = Asset {
            name,
            metadata_size,
            file_size,
            format,
            data_offset,
            tree,
            objects,
            adds,
            refs,
        };
        Ok((input, asset))
    }
}

impl<'b> Asset<'b> {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn objects(&self) -> &[Object<'b>] {
        &self.objects
    }
}
