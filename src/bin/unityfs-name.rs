fn main() {
    let mut args = std::env::args().skip(1);
    let filename = args.next().expect("Expected filename");
    let buf = std::fs::read(filename).expect("Failed to read file");

    let (_, meta) = match unityfs::UnityFsMeta::parse(&buf) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to parse: {:?}", e);
            std::process::exit(1);
        },
    };
    let fs = meta.read_unityfs();
    let asset = fs.main_asset();
    for object in asset.objects() {
        match &object.data {
            unityfs::Data::GenericStruct { type_name, fields } if type_name == "AssetBundle" => {
                match fields.get("m_Name") {
                    Some(unityfs::Data::String(s)) => {
                        println!("{}", String::from_utf8_lossy(s));
                        return;
                    },
                    Some(_) => {
                        eprintln!("m_Name type mismatch");
                        std::process::exit(1);
                    },
                    None => {
                        eprintln!("m_Name not found at AssetBundle");
                        std::process::exit(1);
                    },
                }
            },
            _ => {},
        }
    }
    eprintln!("Cannot find AssetBundle object");
    std::process::exit(2);
}
