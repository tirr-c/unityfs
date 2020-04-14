mod lz4;

use std::cell::{Cell, UnsafeCell};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum CompressionType {
    Lzma,
    Lz4,
    Lz4Hc,
    Lzham,
}

impl CompressionType {
    fn from_u32(val: u32) -> Option<Self> {
        Some(match val {
            0 => return None,
            1 => CompressionType::Lzma,
            2 => CompressionType::Lz4,
            3 => CompressionType::Lz4Hc,
            4 => CompressionType::Lzham,
            _ => return None,
        })
    }

    fn decompress(self, data: &[u8]) -> Vec<u8> {
        use CompressionType::*;

        match self {
            Lz4 | Lz4Hc => lz4::decode_block(data).unwrap(),
            _ => unimplemented!(),
        }
    }
}

pub struct CompressedBlock<'a> {
    u_size: u32,
    compression: Option<CompressionType>,
    block: &'a [u8],
}

impl<'a> CompressedBlock<'a> {
    pub fn from_slice(uncompressed_size: u32, compression_type_id: u32, block: &'a [u8]) -> Self {
        Self {
            u_size: uncompressed_size,
            compression: CompressionType::from_u32(compression_type_id),
            block,
        }
    }
}

impl CompressedBlock<'_> {
    pub fn uncompressed_size(&self) -> u32 {
        self.u_size
    }

    pub fn decompress(&self) -> Vec<u8> {
        match self.compression {
            None => self.block.to_vec(),
            Some(compression) => compression.decompress(self.block),
        }
    }
}

struct BlockEntry<'a> {
    offset: u64,
    uncompressed: Cell<bool>,
    data: CompressedBlock<'a>,
}

pub struct CompressedBlockStorage<'a> {
    blocks: Vec<BlockEntry<'a>>,
    buf: UnsafeCell<Box<[u8]>>,
}

impl<'a> CompressedBlockStorage<'a> {
    pub fn from_blocks(blocks: Vec<CompressedBlock<'a>>) -> Self {
        let mut total_len = 0u64;
        let blocks = blocks
            .into_iter()
            .map(|b| {
                let start_offset = total_len;
                total_len += <_ as Into<u64>>::into(b.uncompressed_size());
                BlockEntry {
                    offset: start_offset,
                    uncompressed: Cell::new(false),
                    data: b,
                }
            })
            .collect();
        let buf = UnsafeCell::new(vec![0; total_len as usize].into());
        Self { blocks, buf }
    }
}

impl CompressedBlockStorage<'_> {
    unsafe fn get_buf_by_entry_mut(&self, entry: &BlockEntry) -> &mut [u8] {
        let start = entry.offset as usize;
        let len = entry.data.uncompressed_size() as usize;
        let end = (start + len) as usize;
        &mut (*self.buf.get())[start..end]
    }

    pub fn read_range(&self, range: std::ops::Range<u64>) -> &[u8] {
        let std::ops::Range { start, end } = range;
        let start_block_idx = self
            .blocks
            .binary_search_by_key(&start, |b| b.offset)
            .unwrap_or_else(|idx| idx - 1);
        let end_block_idx = self
            .blocks
            .binary_search_by_key(&end, |b| b.offset)
            .unwrap_or_else(|idx| idx)
            - 1;
        for entry in &self.blocks[start_block_idx..=end_block_idx] {
            if entry.uncompressed.replace(true) {
                continue;
            }
            let block = entry.data.decompress();
            let buf_area = unsafe { self.get_buf_by_entry_mut(entry) };
            debug_assert_eq!(block.len(), buf_area.len());
            let len = std::cmp::min(block.len(), buf_area.len());
            buf_area[0..len].copy_from_slice(&block[0..len]);
        }

        unsafe { &(*self.buf.get())[start as usize..end as usize] }
    }
}
