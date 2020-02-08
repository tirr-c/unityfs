pub fn align<'a>(offset: usize, base: &'a [u8], target: &'a [u8]) -> &'a [u8] {
    let dist = (target.as_ptr() as usize) - (base.as_ptr() as usize);
    let new = ((offset + dist + 3) & 0xfffffffc) - offset;
    return &base[new..];
}
