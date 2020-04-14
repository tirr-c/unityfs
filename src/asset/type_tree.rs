use crate::common_parser::read_string;
use crate::util::align;
use nom::{
    i16, i32, i64,
    number::{complete as nom_number, Endianness},
    u16, u32, u64, IResult,
};
use serde::Serialize;
use std::borrow::Cow;
use std::collections::HashMap;

#[derive(Debug)]
pub struct TypeTree<'a> {
    version: u32,
    is_array: bool,
    type_name: Cow<'a, str>,
    name: Cow<'a, str>,
    size: u32,
    index: u32,
    flags: u32,
    children: Vec<TypeTree<'a>>,
}

fn parse_old(endianness: Endianness) -> impl Fn(&[u8]) -> IResult<&[u8], TypeTree> {
    move |input| {
        let (input, type_name) = read_string(input, None)?;
        let (input, name) = read_string(input, None)?;
        let (input, size) = u32!(input, endianness)?;
        let (input, index) = u32!(input, endianness)?;
        let (input, is_array) = u32!(input, endianness)?;
        let is_array = is_array != 0;
        let (input, version) = u32!(input, endianness)?;
        let (input, flags) = u32!(input, endianness)?;

        let (input, field_count) = u32!(input, endianness)?;
        let (input, children) =
            nom::multi::count(parse_old(endianness), field_count as usize)(input)?;
        let ret = TypeTree {
            version,
            is_array,
            type_name,
            name,
            size,
            index,
            flags,
            children,
        };
        Ok((input, ret))
    }
}

const STRINGS_DAT: &'static [u8] = include_bytes!("strings.dat");

fn parse_blob(input: &[u8], endianness: Endianness, format: u32) -> IResult<&[u8], TypeTree> {
    let (input, node_count) = u32!(input, endianness)?;
    let (input, buffer_bytes) = u32!(input, endianness)?;
    let node_bytes = if format >= 19 { 32 } else { 24 };
    let (mut node_data, input) = input.split_at((node_count as usize) * node_bytes);
    let (data, input) = input.split_at(buffer_bytes as usize);

    let get_string = |offset: u32| -> IResult<&[u8], Cow<'_, str>> {
        let slice = if offset >= 0x80000000 {
            let offset = (offset & 0x7fffffff) as usize;
            &STRINGS_DAT[offset..]
        } else if offset < data.len() as u32 {
            &data[(offset as usize)..]
        } else {
            return Ok((b"".as_ref(), "(null)".into()));
        };
        if slice.is_empty() {
            Ok((b"".as_ref(), "".into()))
        } else {
            read_string(slice, None)
        }
    };

    let mut tree_stack: Vec<TypeTree> = vec![];
    for _ in 0..node_count {
        let input = node_data;
        let (input, version) = u16!(input, endianness)?;
        let (input, depth) = nom_number::be_u8(input)?;
        while tree_stack.len() > depth as usize {
            let node = tree_stack.pop().unwrap();
            tree_stack.last_mut().unwrap().children.push(node);
        }
        let (input, is_array) = nom_number::be_u8(input)?;
        let is_array = is_array != 0;
        let (input, type_name) = u32!(input, endianness)?;
        let (_, type_name) = get_string(type_name)?;
        let (input, name) = u32!(input, endianness)?;
        let (_, name) = get_string(name)?;
        let (input, size) = u32!(input, endianness)?;
        let (input, index) = u32!(input, endianness)?;
        let (input, flags) = u32!(input, endianness)?;
        let input = &input[node_bytes - 24..];
        let node = TypeTree {
            version: version as u32,
            is_array,
            type_name,
            name,
            size,
            index,
            flags,
            children: Vec::new(),
        };
        tree_stack.push(node);
        node_data = input;
    }
    let mut node = tree_stack.pop().unwrap();
    while let Some(mut parent) = tree_stack.pop() {
        parent.children.push(node);
        node = parent;
    }
    Ok((input, node))
}

impl<'a> TypeTree<'a> {
    fn parse(input: &'a [u8], endianness: Endianness, format: u32) -> IResult<&[u8], Self> {
        if format == 10 || format >= 12 {
            parse_blob(input, endianness, format)
        } else {
            parse_old(endianness)(input)
        }
    }

    fn needs_align(&self) -> bool {
        self.flags & 0x4000 != 0
    }

    pub fn read(
        &self,
        input: &'a [u8],
        endianness: Endianness,
        offset: u64,
    ) -> IResult<&'a [u8], Data<'a>> {
        let base = input;
        let mut needs_align = self.needs_align();
        let (input, data) = if self.type_name == "string" {
            debug_assert_eq!(self.children.len(), 1);
            needs_align |= self.children[0].needs_align();
            let (input, length) = u32!(input, endianness)?;
            let (bytes, input) = input.split_at(length as usize);
            (input, Data::String(bytes.into()))
        } else if self.type_name == "pair" {
            debug_assert_eq!(self.children.len(), 2);
            let (input, fst) = self.children[0].read(input, endianness, offset)?;
            let offset = offset + (input.as_ptr() as usize - base.as_ptr() as usize) as u64;
            let (input, snd) = self.children[1].read(input, endianness, offset)?;
            (input, Data::Pair(Box::new(fst), Box::new(snd)))
        } else if let Some(child) = self.children.get(0).filter(|child| child.is_array) {
            child.read(input, endianness, offset)?
        } else if self.is_array {
            debug_assert_eq!(self.children.len(), 2);
            let element_type = &self.children[1];
            let (input, length) = u32!(input, endianness)?;
            if element_type.type_name == "UInt8" {
                let (bytes, input) = input.split_at(length as usize);
                (input, Data::UInt8Array(bytes.into()))
            } else {
                let mut input = input;
                let v = (0..length)
                    .map(|_| {
                        let offset =
                            offset + (input.as_ptr() as usize - base.as_ptr() as usize) as u64;
                        let (left, data) = element_type.read(input, endianness, offset)?;
                        input = left;
                        Ok(data)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                (input, Data::GenericArray(v))
            }
        } else if self.children.len() == 0 {
            let length = self.size;
            let input = if self.type_name == "float" || self.type_name == "double" {
                align(offset as usize, base, input)
            } else {
                input
            };
            let (data, input) = input.split_at(length as usize);
            let data = match self.type_name.as_ref() {
                "bool" => Data::Bool(data[0] != 0),
                "UInt8" => Data::UInt8(nom_number::be_u8(data)?.1),
                "UInt16" => Data::UInt16(u16!(data, endianness)?.1),
                "UInt32" | "unsigned int" => Data::UInt32(u32!(data, endianness)?.1),
                "UInt64" => Data::UInt64(u64!(data, endianness)?.1),
                "SInt8" => Data::SInt8(nom_number::be_i8(data)?.1),
                "SInt16" => Data::SInt16(i16!(data, endianness)?.1),
                "SInt32" | "int" => Data::SInt32(i32!(data, endianness)?.1),
                "SInt64" => Data::SInt64(i64!(data, endianness)?.1),
                "float" => Data::Float(f32::from_bits(u32!(data, endianness)?.1)),
                "double" => Data::Double(f64::from_bits(u64!(data, endianness)?.1)),
                _ => Data::GenericPrimitive {
                    type_name: self.type_name.clone(),
                    data: data.into(),
                },
            };
            (input, data)
        } else {
            let mut input = input;
            let fields = self
                .children
                .iter()
                .map(|field_type| {
                    let offset = offset + (input.as_ptr() as usize - base.as_ptr() as usize) as u64;
                    let (left, data) = field_type.read(input, endianness, offset)?;
                    input = left;
                    Ok((field_type.name.clone(), data))
                })
                .collect::<Result<HashMap<_, _>, _>>()?;
            (
                input,
                Data::GenericStruct {
                    type_name: self.type_name.clone(),
                    fields,
                },
            )
        };
        let input = if needs_align {
            align(offset as usize, base, input)
        } else {
            input
        };
        Ok((input, data))
    }
}

#[derive(Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Data<'b> {
    GenericPrimitive {
        type_name: Cow<'b, str>,
        data: Cow<'b, [u8]>,
    },
    GenericArray(Vec<Data<'b>>),
    GenericStruct {
        type_name: Cow<'b, str>,
        fields: HashMap<Cow<'b, str>, Data<'b>>,
    },
    Bool(bool),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    SInt8(i8),
    SInt16(i16),
    SInt32(i32),
    SInt64(i64),
    Float(f32),
    Double(f64),
    String(Cow<'b, [u8]>),
    UInt8Array(Cow<'b, [u8]>),
    Pair(Box<Data<'b>>, Box<Data<'b>>),
}

impl std::fmt::Debug for Data<'_> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Data::GenericPrimitive { type_name, data } => write!(fmt, "{}({:?})", type_name, data),
            Data::GenericArray(data) => fmt.debug_list().entries(data).finish(),
            Data::GenericStruct { type_name, fields } => {
                let mut s = fmt.debug_struct(type_name);
                for (k, v) in fields.iter() {
                    s.field(k, v);
                }
                s.finish()
            }
            Data::Bool(data) => write!(fmt, "Bool({:?})", data),
            Data::UInt8(data) => write!(fmt, "UInt8({:?})", data),
            Data::UInt16(data) => write!(fmt, "UInt16({:?})", data),
            Data::UInt32(data) => write!(fmt, "UInt32({:?})", data),
            Data::UInt64(data) => write!(fmt, "UInt64({:?})", data),
            Data::SInt8(data) => write!(fmt, "SInt8({:?})", data),
            Data::SInt16(data) => write!(fmt, "SInt16({:?})", data),
            Data::SInt32(data) => write!(fmt, "SInt32({:?})", data),
            Data::SInt64(data) => write!(fmt, "SInt64({:?})", data),
            Data::Float(data) => write!(fmt, "Float({:?})", data),
            Data::Double(data) => write!(fmt, "Double({:?})", data),
            Data::String(b) => {
                if let Ok(s) = std::str::from_utf8(b) {
                    write!(fmt, "{:?}", s)
                } else {
                    let len = b.len();
                    write!(
                        fmt,
                        "String({} byte{})",
                        len,
                        if len == 1 { "" } else { "s" }
                    )
                }
            }
            Data::UInt8Array(b) => {
                let len = b.len();
                write!(
                    fmt,
                    "Uint8Array({} byte{})",
                    len,
                    if len == 1 { "" } else { "s" }
                )
            }
            Data::Pair(fst, snd) => fmt.debug_tuple("Pair").field(fst).field(snd).finish(),
        }
    }
}

impl Data<'_> {
    pub fn clone_owned(&self) -> Data<'static> {
        match self {
            Data::Pair(f, s) => Data::Pair(Box::new(f.clone_owned()), Box::new(s.clone_owned())),
            Data::UInt8Array(b) => Data::UInt8Array(b.clone().into_owned().into()),
            Data::String(b) => Data::String(b.clone().into_owned().into()),
            Data::GenericArray(v) => Data::GenericArray(v.iter().map(Self::clone_owned).collect()),
            Data::GenericStruct { type_name, fields } => Data::GenericStruct {
                type_name: type_name.clone().into_owned().into(),
                fields: fields
                    .iter()
                    .map(|(k, v)| (k.clone().into_owned().into(), v.clone_owned()))
                    .collect(),
            },
            Data::GenericPrimitive { type_name, data } => Data::GenericPrimitive {
                type_name: type_name.clone().into_owned().into(),
                data: data.clone().into_owned().into(),
            },
            Data::Bool(v) => Data::Bool(*v),
            Data::UInt8(v) => Data::UInt8(*v),
            Data::UInt16(v) => Data::UInt16(*v),
            Data::UInt32(v) => Data::UInt32(*v),
            Data::UInt64(v) => Data::UInt64(*v),
            Data::SInt8(v) => Data::SInt8(*v),
            Data::SInt16(v) => Data::SInt16(*v),
            Data::SInt32(v) => Data::SInt32(*v),
            Data::SInt64(v) => Data::SInt64(*v),
            Data::Float(v) => Data::Float(*v),
            Data::Double(v) => Data::Double(*v),
        }
    }
}

#[derive(Debug)]
struct TypeMetadataEntry<'a> {
    class_id: i32,
    hash: Option<&'a [u8]>,
    tree: Option<TypeTree<'a>>,
}

impl<'a> TypeMetadataEntry<'a> {
    fn parse(
        input: &'a [u8],
        endianness: Endianness,
        format: u32,
        has_type_trees: bool,
    ) -> IResult<&'a [u8], Self> {
        let (input, class_id) = i32!(input, endianness)?;
        let (input, class_id) = if format >= 17 {
            let input = &input[1..];
            let (input, script_id) = i16!(input, endianness)?;
            let script_id: i32 = script_id.into();
            let class_id = if class_id == 114 {
                if script_id >= 0 {
                    -2 - script_id
                } else {
                    -1
                }
            } else {
                class_id
            };
            (input, class_id)
        } else {
            (input, class_id)
        };
        let (hash, input) = if class_id < 0 {
            input.split_at(0x20)
        } else {
            input.split_at(0x10)
        };
        let (input, tree) = if has_type_trees {
            let (input, tree) = TypeTree::parse(input, endianness, format)?;
            (input, Some(tree))
        } else {
            (input, None)
        };
        Ok((
            input,
            Self {
                class_id,
                hash: Some(hash),
                tree,
            },
        ))
    }

    fn parse_old(input: &'a [u8], endianness: Endianness, format: u32) -> IResult<&'a [u8], Self> {
        let (input, class_id) = i32!(input, endianness)?;
        let (input, tree) = TypeTree::parse(input, endianness, format)?;
        Ok((
            input,
            Self {
                class_id,
                hash: None,
                tree: Some(tree),
            },
        ))
    }
}

#[derive(Debug)]
pub struct TypeMetadata<'a> {
    generator_version: Cow<'a, str>,
    target_platform: u32,
    class_ids: Vec<i32>,
    entries: HashMap<i32, TypeMetadataEntry<'a>>,
}

impl<'a> TypeMetadata<'a> {
    pub fn parse(input: &'a [u8], endianness: Endianness, format: u32) -> IResult<&'a [u8], Self> {
        let (input, generator_version) = read_string(input, None)?;
        let (input, target_platform) = u32!(input, endianness)?;

        let (input, entries) = if format >= 13 {
            let has_type_trees = input[0] != 0;
            let input = &input[1..];
            let (mut input, num_types) = u32!(input, endianness)?;

            let entries = (0..num_types)
                .map(|_| {
                    let (left, entry) =
                        TypeMetadataEntry::parse(input, endianness, format, has_type_trees)?;
                    input = left;
                    Ok(entry)
                })
                .collect::<Result<Vec<_>, _>>()?;
            (input, entries)
        } else {
            let (mut input, fields_count) = u32!(input, endianness)?;
            let entries = (0..fields_count)
                .map(|_| {
                    let (left, entry) = TypeMetadataEntry::parse_old(input, endianness, format)?;
                    input = left;
                    Ok(entry)
                })
                .collect::<Result<Vec<_>, _>>()?;
            (input, entries)
        };
        let class_ids = entries.iter().map(|entry| entry.class_id).collect();
        let entries = entries
            .into_iter()
            .map(|entry| (entry.class_id, entry))
            .collect();

        Ok((
            input,
            Self {
                generator_version,
                target_platform,
                class_ids,
                entries,
            },
        ))
    }

    pub fn class_id_from_idx(&self, idx: usize) -> i32 {
        self.class_ids[idx]
    }

    pub fn type_tree_from_id(&self, type_id: i32, class_id: i32) -> Option<&TypeTree<'a>> {
        self.entries
            .get(&type_id)
            .or_else(|| DEFAULT_TYPES.entries.get(&class_id))
            .and_then(|entry| entry.tree.as_ref())
    }
}

const DEFAULT_STRUCTS: &'static [u8] = include_bytes!("structs.dat");

lazy_static::lazy_static! {
    static ref DEFAULT_TYPES: TypeMetadata<'static> = {
        TypeMetadata::parse(DEFAULT_STRUCTS, Endianness::Little, 15).unwrap().1
    };
}
