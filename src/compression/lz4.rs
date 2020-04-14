const MIN_MATCH_LEN: usize = 4;

#[derive(Debug)]
struct Sequences<'a> {
    input: &'a [u8],
    output_length: usize,
}

impl<'a> Sequences<'a> {
    fn new(input: &'a [u8]) -> Sequences {
        Self {
            input,
            output_length: 0,
        }
    }
}

#[derive(Debug)]
struct Sequence<'a> {
    literal: &'a [u8],
    match_copy: Option<MatchCopyInfo>,
}

#[derive(Copy, Clone, Debug)]
struct MatchCopyInfo {
    offset: u16,
    length: usize,
}

impl Sequence<'_> {
    fn len(&self) -> usize {
        self.literal.len() + self.match_copy.map(|v| v.length).unwrap_or(0)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Lz4Error {
    // #[fail(display = "unexpected end of input")]
    UnexpectedEnd,
    // #[fail(display = "invalid lookback offset")]
    InvalidLookback,
}

impl std::fmt::Display for Lz4Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Lz4Error::UnexpectedEnd => "unexpected end of input",
            Lz4Error::InvalidLookback => "invalid lookback offset",
        };
        f.write_str(s)
    }
}

impl std::error::Error for Lz4Error {}

impl<'a> Iterator for Sequences<'a> {
    type Item = Result<Sequence<'a>, Lz4Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut input = self.input;
        if input.is_empty() {
            return None;
        }

        let mut lit_len = ((input[0] & 0xf0) >> 4) as usize;
        let mut match_len = (input[0] & 0x0f) as usize;
        input = &input[1..];
        if lit_len == 0x0f {
            let count = input.iter().position(|&b| b != 0xff);
            let count = match count {
                Some(count) => count,
                None => return Some(Err(Lz4Error::UnexpectedEnd)),
            };
            lit_len += 0xff * count + input[count] as usize;
            input = &input[1 + count..];
        }
        let lit = &input[0..lit_len];
        input = &input[lit_len..];
        self.output_length += lit_len;
        if input.is_empty() {
            self.input = input;
            return Some(Ok(Sequence {
                literal: lit,
                match_copy: None,
            }));
        }

        if input.len() < 2 {
            return Some(Err(Lz4Error::UnexpectedEnd));
        }
        let offset = u16::from_le_bytes([input[0], input[1]]);
        if offset == 0 || offset as usize > self.output_length {
            return Some(Err(Lz4Error::InvalidLookback));
        }
        input = &input[2..];
        if match_len == 0x0f {
            let count = input.iter().position(|&b| b != 0xff)?;
            match_len += 0xff * count + input[count] as usize;
            input = &input[1 + count..];
        }
        self.input = input;
        self.output_length += match_len + MIN_MATCH_LEN;
        Some(Ok(Sequence {
            literal: lit,
            match_copy: Some(MatchCopyInfo {
                offset,
                length: match_len + MIN_MATCH_LEN,
            }),
        }))
    }
}

pub fn decode_block(input: &[u8]) -> Result<Vec<u8>, Lz4Error> {
    let seq = Sequences::new(input);
    let uncompressed_len = seq
        .map(|seq| seq.map(|seq| seq.len()))
        .sum::<Result<usize, _>>()?;
    let mut out = vec![0u8; uncompressed_len];
    let mut p = 0;

    for seq in Sequences::new(input) {
        let seq = seq?;
        out[p..p + seq.literal.len()].copy_from_slice(seq.literal);
        p += seq.literal.len();
        if let Some(match_copy) = seq.match_copy {
            let offset = match_copy.offset as usize;
            let (match_bytes, out_buf) = out[p - offset..].split_at_mut(offset);
            let out_buf = &mut out_buf[0..match_copy.length];
            for out_buf in out_buf.chunks_mut(match_bytes.len()) {
                out_buf.copy_from_slice(&match_bytes[0..out_buf.len()]);
            }
            p += match_copy.length;
        }
    }
    Ok(out)
}
