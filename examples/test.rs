fn main() {
    let mut args = std::env::args().skip(1);
    let filename = args.next().expect("Expected filename");
    let buf = std::fs::read(filename).expect("Failed to read file");

    let (_, meta) = unityfs::UnityFsMeta::parse(&buf).unwrap();
    let fs = meta.read_unityfs();
    println!("{:#?}", fs.main_asset().objects());
}
