use std::borrow::Cow;
use nom::{
    bytes::complete as nom_bytes,
    sequence as nom_sequence,
    IResult,
};

pub fn read_string(input: &[u8], length: Option<usize>) -> IResult<&[u8], Cow<'_, str>> {
    let (input, s) = match length {
        Some(length) => nom_bytes::take(length)(input),
        None => nom_sequence::terminated(nom_bytes::take_until([0].as_ref()), nom_bytes::tag(&[0]))(input),
    }?;
    Ok((input, String::from_utf8_lossy(s)))
}
