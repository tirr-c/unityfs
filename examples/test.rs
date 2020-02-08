fn main() {
    let mut args = std::env::args().skip(1);
    let filename = args.next().expect("Expected filename");
    let buf = std::fs::read(filename).expect("Failed to read file");

    let (left, (_fsmeta, block)) = unityfs::read_unityfs_meta(&buf).unwrap();
    let block = block.decompress();
    let (_, metadata) = unityfs::Metadata::parse(&block).unwrap();
    let block_storage = unityfs::read_blocks(left, &metadata);
    let fs = unityfs::read_unityfs(metadata, &block_storage);
    println!("{:#?}", fs.assets()[0].objects());
}
