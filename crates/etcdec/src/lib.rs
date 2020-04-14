const TABLE_59T: [u8; 8] = [3, 6, 11, 16, 23, 32, 41, 64];
const TABLE_58H: [u8; 8] = [3, 6, 11, 16, 23, 32, 41, 64];
const COMPRESS_PARAMS: [[i32; 4]; 16] = [
    [-8, -2, 2, 8],
    [-8, -2, 2, 8],
    [-17, -5, 5, 17],
    [-17, -5, 5, 17],
    [-29, -9, 9, 29],
    [-29, -9, 9, 29],
    [-42, -13, 13, 42],
    [-42, -13, 13, 42],
    [-60, -18, 18, 60],
    [-60, -18, 18, 60],
    [-80, -24, 24, 80],
    [-80, -24, 24, 80],
    [-106, -33, 33, 106],
    [-106, -33, 33, 106],
    [-183, -47, 47, 183],
    [-183, -47, 47, 183],
];
const UNSCRAMBLE: [usize; 4] = [2, 3, 1, 0];
const ALPHA_BASE: [[i32; 4]; 16] = [
    [-15, -9, -6, -3],
    [-13, -10, -7, -3],
    [-13, -8, -5, -2],
    [-13, -6, -4, -2],
    [-12, -8, -6, -3],
    [-11, -9, -7, -3],
    [-11, -8, -7, -4],
    [-11, -8, -5, -3],
    [-10, -8, -6, -2],
    [-10, -8, -5, -2],
    [-10, -8, -4, -2],
    [-10, -7, -5, -2],
    [-10, -7, -4, -3],
    [-10, -3, -2, -1],
    [-9, -8, -6, -4],
    [-9, -7, -5, -3],
];
lazy_static::lazy_static! {
    static ref ALPHA_TABLE: [[i32; 8]; 256] = {
        let mut table = [[0; 8]; 256];
        for i in 0..16 {
            for j in 0..4 {
                let data = ALPHA_BASE[i][3 - j % 4];
                table[i + 16][j] = data;
                table[i + 16][j + 4] = -data - 1;
            }
        }
        let (base, target) = table.split_at_mut(32);
        let base = &mut base[16..];
        for i in 0..224 {
            let mul = 2 + (i / 16) as i32;
            let source = &base[i % 16];
            let target = &mut target[i];
            for (target, source) in target.iter_mut().zip(source.iter()) {
                *target = *source * mul;
            }
        }
        table
    };
}

#[inline]
fn saturating_add_u8_i32(a: u8, b: i32) -> u8 {
    let x = i32::from(a) + b;
    if x < 0 {
        0
    } else if x > 0xff {
        0xff
    } else {
        x as u8
    }
}

#[inline]
fn extend_u8_3bit(v: u8) -> u8 {
    let v = v as i8;
    ((v << 5) >> 5) as u8
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Pattern {
    H = 0,
    T = 1,
}

macro_rules! extract_and_shift {
    ($v:ident >> $w:expr) => {{
        let mask = (1 << $w) - 1;
        let ret = $v & mask;
        $v >>= $w;
        ret
    }};
}

fn unstuff_57(mut from: u64) -> u64 {
    let a = extract_and_shift!(from >> 33);
    from >>= 1;
    let b = extract_and_shift!(from >> 8);
    from >>= 1;
    let c = extract_and_shift!(from >> 2);
    from >>= 3;
    let d = extract_and_shift!(from >> 7);
    from >>= 1;
    let e = extract_and_shift!(from >> 7);
    drop(from);

    (e << 57) | (d << 50) | (c << 48) | (b << 40) | (a << 7)
}

fn unstuff_58(mut from: u64) -> u64 {
    let a = extract_and_shift!(from >> 33);
    from >>= 1;
    let b = extract_and_shift!(from >> 16);
    from >>= 1;
    let c = extract_and_shift!(from >> 2);
    from >>= 3;
    let d = extract_and_shift!(from >> 7);
    drop(from);

    (d << 51) | (c << 49) | (b << 33) | a
}

fn unstuff_59(mut from: u64) -> u64 {
    let a = extract_and_shift!(from >> 33);
    from >>= 1;
    let b = extract_and_shift!(from >> 24);
    from >>= 1;
    let c = extract_and_shift!(from >> 2);
    drop(from);

    (c << 57) | (b << 33) | a
}

fn decompress_color(rb: usize, gb: usize, bb: usize, rgb444: [[u8; 3]; 2]) -> [[u8; 3]; 2] {
    #[inline(always)]
    fn calc(v: u8, b: usize) -> u8 {
        (v << (8 - b)) | (v >> (b - (8 - b)))
    }

    [
        [
            calc(rgb444[0][0], rb),
            calc(rgb444[0][1], gb),
            calc(rgb444[0][2], bb),
        ],
        [
            calc(rgb444[1][0], rb),
            calc(rgb444[1][1], gb),
            calc(rgb444[1][2], bb),
        ],
    ]
}

fn calculate_paint_colors(dist: u8, pat: Pattern, colors: [[u8; 3]; 2]) -> [[u8; 3]; 4] {
    match pat {
        Pattern::H => {
            let v = TABLE_58H[usize::from(dist)];
            [
                [
                    colors[0][0].saturating_add(v),
                    colors[0][1].saturating_add(v),
                    colors[0][2].saturating_add(v),
                ],
                [
                    colors[0][0].saturating_sub(v),
                    colors[0][1].saturating_sub(v),
                    colors[0][2].saturating_sub(v),
                ],
                [
                    colors[1][0].saturating_add(v),
                    colors[1][1].saturating_add(v),
                    colors[1][2].saturating_add(v),
                ],
                [
                    colors[1][0].saturating_sub(v),
                    colors[1][1].saturating_sub(v),
                    colors[1][2].saturating_sub(v),
                ],
            ]
        }
        Pattern::T => {
            let v = TABLE_59T[usize::from(dist)];
            [
                colors[0],
                [
                    colors[1][0].saturating_add(v),
                    colors[1][1].saturating_add(v),
                    colors[1][2].saturating_add(v),
                ],
                colors[1],
                [
                    colors[1][0].saturating_sub(v),
                    colors[1][1].saturating_sub(v),
                    colors[1][2].saturating_sub(v),
                ],
            ]
        }
    }
}

const CHANNELS: usize = 4;
const BLOCK_WIDTH: usize = 4;
const BLOCK_HEIGHT: usize = 4;
type Block = [[u8; BLOCK_WIDTH * CHANNELS]; BLOCK_HEIGHT];
type SingleChannelBlock = [[u8; BLOCK_WIDTH]; BLOCK_HEIGHT];

fn decompress_block_thumb(block: u64, pattern: Pattern, alpha: bool) -> Block {
    let mut color_data = block >> 32;
    let dist = match pattern {
        Pattern::H => {
            let col0 = (color_data >> 14) & 0x0fff;
            let col1 = (color_data >> 2) & 0x0fff;
            let extra_bit = if col0 >= col1 { 1 } else { 0 };
            extract_and_shift!(color_data >> 2) as u8 | extra_bit
        }
        Pattern::T => extract_and_shift!(color_data >> 3) as u8,
    };
    let rgb444 = {
        let mut ret = [[0u8; 3]; 2];
        for idx in 0..2 {
            for color in 0..3 {
                ret[1 - idx][2 - color] = extract_and_shift!(color_data >> 4) as u8;
            }
        }
        ret
    };
    let colors = decompress_color(4, 4, 4, rgb444);
    let paint_colors = calculate_paint_colors(dist, pattern, colors);

    let mut idx_upper = ((block >> 16) & 0xffff) as usize;
    let mut idx_lower = (block & 0xffff) as usize;
    let mut ret = [[0u8; BLOCK_WIDTH * CHANNELS]; BLOCK_HEIGHT];
    for x in 0..BLOCK_WIDTH {
        for y in 0..BLOCK_HEIGHT {
            let start_offset = CHANNELS * x;
            let slice = &mut ret[y][start_offset..start_offset + 4];

            let idx =
                (extract_and_shift!(idx_upper >> 1) << 1) | extract_and_shift!(idx_lower >> 1);
            let color = &paint_colors[idx];
            slice[0..3].copy_from_slice(color);
            slice[3] = if alpha && idx == 2 { 0 } else { 255 };
        }
    }
    ret
}

fn decompress_block_planar(mut block: u64) -> Block {
    block >>= 7;
    let colors = {
        let mut ret = [[0u8; 3]; 3];
        for color in ret.iter_mut() {
            color[2] = extract_and_shift!(block >> 6) as u8;
            color[2] = (color[2] << 2) | (color[2] >> 4);
            color[1] = extract_and_shift!(block >> 7) as u8;
            color[1] = (color[1] << 1) | (color[1] >> 6);
            color[0] = extract_and_shift!(block >> 6) as u8;
            color[0] = (color[0] << 2) | (color[0] >> 4);
        }
        ret
    };
    let v = &colors[0];
    let h = &colors[1];
    let o = &colors[2];
    let mut ret = [[0u8; BLOCK_WIDTH * CHANNELS]; BLOCK_HEIGHT];
    for x in 0..BLOCK_WIDTH {
        for y in 0..BLOCK_HEIGHT {
            for c in 0..3 {
                let xx = x as isize;
                let yy = y as isize;
                let h = h[c] as isize;
                let v = v[c] as isize;
                let o = o[c] as isize;
                let val = (xx * (h - o) + yy * (v - o) + 4 * o + 2) >> 2;
                let val = if val < 0 {
                    0
                } else if val > 0xff {
                    0xff
                } else {
                    val as u8
                };
                ret[y][CHANNELS * x + c] = val;
            }
            ret[y][CHANNELS * x + 3] = 0xff;
        }
    }
    ret
}

fn decompress_block_diff_flip(mut block: u64, alpha: bool) -> Block {
    fn fill(
        colors_table: [([u8; 3], usize); 2],
        idx: u32,
        flip: bool,
        has_transparent: bool,
    ) -> Block {
        let mut map = [[&colors_table[0]; BLOCK_WIDTH]; BLOCK_HEIGHT];
        let (range_x, range_y) = if flip { (0..4, 2..4) } else { (2..4, 0..4) };
        for x in range_x {
            for y in range_y.clone() {
                map[y][x] = &colors_table[1];
            }
        }
        let idx_upper = (idx >> 16) as usize;
        let idx_lower = (idx & 0xffff) as usize;
        let mut ret = [[0u8; BLOCK_WIDTH * CHANNELS]; BLOCK_HEIGHT];
        for y in 0..BLOCK_HEIGHT {
            for x in 0..BLOCK_WIDTH {
                let (color, table) = map[y][x];
                let bit = x * BLOCK_HEIGHT + y;
                let idx = UNSCRAMBLE[(((idx_upper >> bit) & 1) << 1) | ((idx_lower >> bit) & 1)];
                let param = if has_transparent && (idx == 1 || idx == 2) {
                    0
                } else {
                    COMPRESS_PARAMS[*table][idx]
                };
                let base_offset = CHANNELS * x;
                ret[y][base_offset + 0] = saturating_add_u8_i32(color[0], param);
                ret[y][base_offset + 1] = saturating_add_u8_i32(color[1], param);
                ret[y][base_offset + 2] = saturating_add_u8_i32(color[2], param);
                ret[y][base_offset + 3] = if has_transparent && idx == 1 { 0 } else { 0xff };
            }
        }
        ret
    }

    let idx = extract_and_shift!(block >> 32) as u32;
    let flip = extract_and_shift!(block >> 1) != 0;
    let diff = extract_and_shift!(block >> 1) != 0;
    let colors_table = if diff || alpha {
        let table_diff = extract_and_shift!(block >> 3) << 1;
        let table = extract_and_shift!(block >> 3) << 1;
        let mut colors_table = [([0u8; 3], table as usize), ([0u8; 3], table_diff as usize)];
        for i in (0..3).rev() {
            let diff = extend_u8_3bit(extract_and_shift!(block >> 3) as u8);
            let enc_color = extract_and_shift!(block >> 5) as u8;
            let avg_color = (enc_color << 3) | (enc_color >> 2);
            (colors_table[0].0)[i] = avg_color;
            let enc_color = enc_color.overflowing_add(diff).0 as u8;
            let avg_color = (enc_color << 3) | (enc_color >> 2);
            (colors_table[1].0)[i] = avg_color;
        }
        colors_table
    } else {
        let table1 = extract_and_shift!(block >> 3) << 1;
        let table0 = extract_and_shift!(block >> 3) << 1;
        let mut colors_table = [([0u8; 3], table0 as usize), ([0u8; 3], table1 as usize)];
        for i in (0..3).rev() {
            let color = extract_and_shift!(block >> 4) as u8;
            let color = (color << 4) | color;
            (colors_table[1].0)[i] = color;
            let color = extract_and_shift!(block >> 4) as u8;
            let color = (color << 4) | color;
            (colors_table[0].0)[i] = color;
        }
        colors_table
    };
    fill(colors_table, idx, flip, alpha && !diff)
}

fn decompress_block_etc2(block: u64, alpha: bool) -> Block {
    let upper = block >> 32;
    let diff = upper & 2 != 0;
    if diff || alpha {
        let mut color_data = upper >> 8;
        let mut colors = [0, 0, 0];
        for color in colors.iter_mut().rev() {
            let diff = extend_u8_3bit(extract_and_shift!(color_data >> 3) as u8);
            let raw = extract_and_shift!(color_data >> 5) as u8;
            *color = raw.overflowing_add(diff).0;
        }
        if colors[0] >= 32 {
            let block = unstuff_59(block);
            decompress_block_thumb(block, Pattern::T, alpha && !diff)
        } else if colors[1] >= 32 {
            let block = unstuff_58(block);
            decompress_block_thumb(block, Pattern::H, alpha && !diff)
        } else if colors[2] >= 32 {
            let block = unstuff_57(block);
            decompress_block_planar(block)
        } else {
            decompress_block_diff_flip(block, alpha)
        }
    } else {
        decompress_block_diff_flip(block, false)
    }
}

fn decompress_block_alpha(block: u64) -> SingleChannelBlock {
    let alpha = ((block & (0xff << 56)) >> 56) as u8;
    let table = ((block & (0xff << 48)) >> 48) as usize;
    let mut bits = (block & 0xffffffffffff).reverse_bits() >> 16;
    let mut ret = [[0u8; BLOCK_WIDTH]; BLOCK_HEIGHT];
    for x in 0..4 {
        for y in 0..4 {
            let idx = extract_and_shift!(bits >> 3) as u8;
            let idx = idx.reverse_bits() >> 5;
            ret[y][x] = saturating_add_u8_i32(alpha, ALPHA_TABLE[table][idx as usize]);
        }
    }
    ret
}

fn combine_color_alpha(color: Block, alpha: SingleChannelBlock) -> Block {
    let mut ret = color;
    for y in 0..BLOCK_HEIGHT {
        for x in 0..BLOCK_WIDTH {
            ret[y][CHANNELS * x + 3] = alpha[y][x];
        }
    }
    ret
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DecodeFormat {
    EtcRgb4,
    Etc2Rgb,
    Etc2Rgba8,
    Etc2Rgba1,
}

pub fn decode_single_block<R: std::io::Read>(
    input: &mut R,
    format: DecodeFormat,
) -> std::io::Result<Block> {
    let mut buf = [0u8; 8];
    let alpha_block = if format == DecodeFormat::Etc2Rgba8 {
        input.read_exact(&mut buf)?;
        let block = u64::from_be_bytes(buf);
        Some(decompress_block_alpha(block))
    } else {
        None
    };
    let has_1bit_alpha = format == DecodeFormat::Etc2Rgba1;
    let color_block = {
        input.read_exact(&mut buf)?;
        let block = u64::from_be_bytes(buf);
        decompress_block_etc2(block, has_1bit_alpha)
    };
    let ret = if let Some(alpha_block) = alpha_block {
        combine_color_alpha(color_block, alpha_block)
    } else {
        color_block
    };
    Ok(ret)
}
