use wasm_bindgen::prelude::*;
use js_sys::{Array, Object, Reflect, Uint8Array, Uint8ClampedArray};

use image::dxt;

#[wasm_bindgen]
pub struct UnityFs {
    input: Vec<u8>,
}

#[wasm_bindgen]
impl UnityFs {
    pub fn load(input: Vec<u8>) -> UnityFs {
        console_error_panic_hook::set_once();
        Self { input }
    }

    #[wasm_bindgen(getter, js_name = mainAsset)]
    pub fn main_asset(&self) -> Result<Object, JsValue> {
        let (_, meta) = unityfs::UnityFsMeta::parse(&self.input).map_err(|e| JsValue::from(format!("parse failed: {:?}", e)))?;
        let fs = meta.read_unityfs();

        let asset = fs.main_asset();
        let name = asset.name();
        let objects = asset
            .objects()
            .iter()
            .map(|obj| convert_data(&obj.data))
            .collect::<Result<Array, _>>()?;
        let obj = Object::new();
        Reflect::set(&obj, &"name".into(), &name.into())?;
        Reflect::set(&obj, &"objects".into(), &objects.into())?;
        Ok(obj)
    }
}

#[wasm_bindgen]
pub struct Texture2D {
    name: String,
    #[wasm_bindgen(readonly)]
    pub width: u32,
    #[wasm_bindgen(readonly)]
    pub height: u32,
    image_data: ImageData,
}

struct StreamingInfo {
    path: String,
    offset: u32,
    size: u32,
}

impl StreamingInfo {
    fn from_data(data: &unityfs::Data<'_>) -> Result<Self, JsValue> {
        let fields = match data {
            unityfs::Data::GenericStruct { type_name, fields } if type_name == "StreamingInfo" => {
                fields
            },
            _ => return Err(JsValue::from("type mismatch")),
        };
        let path = match fields.get("path") {
            Some(unityfs::Data::String(s)) => {
                String::from_utf8_lossy(s).into_owned()
            },
            _ => return Err(JsValue::from("type mismatch")),
        };
        let offset = match fields.get("offset") {
            Some(unityfs::Data::UInt32(v)) => *v,
            _ => return Err(JsValue::from("type mismatch")),
        };
        let size = match fields.get("size") {
            Some(unityfs::Data::UInt32(v)) => *v,
            _ => return Err(JsValue::from("type mismatch")),
        };
        Ok(Self {
            path,
            offset,
            size,
        })
    }
}

enum ImageData {
    Loaded(Vec<u8>),
    Streaming(DecodeFormat, StreamingInfo),
}

#[derive(Copy, Clone)]
enum DecodeFormat {
    Etc(etcdec::DecodeFormat),
    Dxt(dxt::DXTVariant),
}

impl Texture2D {
    fn read_etc(
        width: u32,
        height: u32,
        format: etcdec::DecodeFormat,
        mut image_data: impl std::io::Read,
    ) -> Result<Vec<u8>, JsValue> {
        let block_width = (width + 3) / 4;
        let block_height = (height + 3) / 4;
        let scanline = (width * 4) as usize;
        let mut buf = vec![0u8; scanline * height as usize];
        for block_y in 0..block_height {
            let y = block_y * 4;
            for block_x in 0..block_width {
                let x = block_x * 4;
                let block = etcdec::decode_single_block(&mut image_data, format).map_err(|_| JsValue::from("read error"))?;
                for (block_raw, target) in block.iter().zip(buf[(4 * x as usize)..].chunks_mut(scanline).rev().skip(y as usize).take(4)) {
                    target[..16].copy_from_slice(block_raw);
                }
            }
        }
        Ok(buf)
    }

    fn read_dxt(
        width: u32,
        height: u32,
        variant: dxt::DXTVariant,
        image_data: impl std::io::Read,
    ) -> Result<Vec<u8>, JsValue> {
        let dec = dxt::DxtDecoder::new(image_data, width, height, variant).map_err(|e| JsValue::from(format!("failed to build decoder: {}", e)))?;
        let image = image::DynamicImage::from_decoder(dec).map_err(|e| JsValue::from(format!("failed to decode: {}", e)))?;
        let image = image.flipv().into_rgba();
        Ok(image.into_vec())
    }

    fn read(
        width: u32,
        height: u32,
        format: DecodeFormat,
        image_data: impl std::io::Read,
    ) -> Result<Vec<u8>, JsValue> {
        match format {
            DecodeFormat::Etc(format) => Self::read_etc(width, height, format, image_data),
            DecodeFormat::Dxt(variant) => Self::read_dxt(width, height, variant, image_data),
        }
    }

    fn load(
        name: String,
        width: u32,
        height: u32,
        format: DecodeFormat,
        image_data: impl std::io::Read,
    ) -> Result<Self, JsValue> {
        let image_data = Texture2D::read(width, height, format, image_data)?;
        Ok(Self {
            name,
            width,
            height,
            image_data: ImageData::Loaded(image_data),
        })
    }

    fn defer(
        name: String,
        width: u32,
        height: u32,
        format: DecodeFormat,
        streaming_info: StreamingInfo,
    ) -> Self {
        Self {
            name,
            width,
            height,
            image_data: ImageData::Streaming(format, streaming_info),
        }
    }
}

#[wasm_bindgen]
impl Texture2D {
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn image_rgba8(&self) -> Option<Uint8ClampedArray> {
        match &self.image_data {
            ImageData::Loaded(data) => Some((&**data).into()),
            _ => None,
        }
    }

    pub fn encode(&self) -> Result<Option<Vec<u8>>, JsValue> {
        let image_data = match &self.image_data {
            ImageData::Loaded(data) => &**data,
            _ => return Ok(None),
        };
        let mut buf = Vec::new();
        let w = std::io::BufWriter::new(&mut buf);
        let mut encoder = png::Encoder::new(w, self.width, self.height);
        encoder.set_compression(png::Compression::Fast);
        encoder.set_color(png::ColorType::RGBA);
        encoder.set_depth(png::BitDepth::Eight);
        let mut w = encoder.write_header().map_err(|e| JsValue::from(format!("error initializing encoder: {}", e)))?;
        w.write_image_data(image_data).map_err(|e| JsValue::from(format!("error while encoding: {}", e)))?;
        drop(w);
        Ok(Some(buf))
    }

    #[wasm_bindgen(js_name = assetDependency)]
    pub fn asset_dependency(&self) -> Option<String> {
        match &self.image_data {
            ImageData::Streaming(_, StreamingInfo { path, .. }) => Some(path.clone()),
            _ => None,
        }
    }

    #[wasm_bindgen(js_name = tryResolve)]
    pub fn try_resolve(&mut self, fs: &UnityFs) -> Result<(), JsValue> {
        let (_, meta) = unityfs::UnityFsMeta::parse(&fs.input).map_err(|e| JsValue::from(format!("parse failed: {:?}", e)))?;
        let fs = meta.read_unityfs();

        let (format, streaming_info) = match &self.image_data {
            ImageData::Streaming(format, val) => (format, val),
            _ => return Ok(()),
        };
        if !streaming_info.path.starts_with("archive:/") {
            return Ok(());
        }
        let mut path_segments = streaming_info.path[9..].split('/');
        let (bundle_name, resource_name) = match (path_segments.next(), path_segments.next()) {
            (Some(x), Some(y)) => (x, y),
            _ => return Ok(()),
        };
        if fs.name() != bundle_name {
            return Ok(());
        }
        let resource = if let Some(buf) = fs.resource(resource_name) {
            buf
        } else {
            return Ok(());
        };
        let buf = &resource[streaming_info.offset as usize..][..streaming_info.size as usize];
        let image_data = Texture2D::read(self.width, self.height, *format, std::io::Cursor::new(buf))?;
        self.image_data = ImageData::Loaded(image_data);
        Ok(())
    }
}

fn convert_data(data: &unityfs::Data<'_>) -> Result<JsValue, JsValue> {
    use unityfs::Data;

    Ok(match data {
        Data::GenericPrimitive { type_name, data } => {
            let obj = Object::new();
            Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str(type_name))?;
            Reflect::set(&obj, &JsValue::from_str("data"), &Uint8Array::from(&**data))?;
            obj.into()
        },
        Data::GenericStruct { type_name, fields } => {
            let data = if type_name == "Texture2D" {
                let name = match fields.get("m_Name") {
                    Some(Data::String(s)) => String::from_utf8_lossy(s).into_owned(),
                    Some(_) => return Err("m_Name type mismatch".into()),
                    None => return Err("m_Name not found".into()),
                };
                let width = match fields.get("m_Width") {
                    Some(Data::SInt32(width)) => (*width) as u32,
                    Some(_) => return Err("m_Width type mismatch".into()),
                    None => return Err("m_Width not found".into()),
                };
                let height = match fields.get("m_Height") {
                    Some(Data::SInt32(height)) => (*height) as u32,
                    Some(_) => return Err("m_Height type mismatch".into()),
                    None => return Err("m_Height not found".into()),
                };
                let image_data = match fields.get("image data") {
                    Some(Data::UInt8Array(buf)) => buf,
                    Some(_) => return Err("image data type mismatch".into()),
                    None => return Err("image data not found".into()),
                };
                let image_data = std::io::Cursor::new(image_data);
                let format = match fields.get("m_TextureFormat") {
                    Some(Data::SInt32(34)) => DecodeFormat::Etc(etcdec::DecodeFormat::EtcRgb4),
                    Some(Data::SInt32(45)) => DecodeFormat::Etc(etcdec::DecodeFormat::Etc2Rgb),
                    Some(Data::SInt32(46)) => DecodeFormat::Etc(etcdec::DecodeFormat::Etc2Rgba1),
                    Some(Data::SInt32(47)) => DecodeFormat::Etc(etcdec::DecodeFormat::Etc2Rgba8),
                    Some(Data::SInt32(10)) => DecodeFormat::Dxt(dxt::DXTVariant::DXT1),
                    Some(Data::SInt32(12)) => DecodeFormat::Dxt(dxt::DXTVariant::DXT5),
                    Some(Data::SInt32(_)) => return Err("unknown texture format".into()),
                    Some(_) => return Err("m_TextureFormat type mismatch".into()),
                    None => return Err("m_TextureFormat not found".into()),
                };
                let streaming_info = fields.get("m_StreamData")
                    .ok_or_else(|| JsValue::from("m_StreamData not found"))
                    .and_then(StreamingInfo::from_data)?;
                if streaming_info.path.is_empty() {
                    Texture2D::load(name, width, height, format, image_data)?.into()
                } else {
                    Texture2D::defer(name, width, height, format, streaming_info).into()
                }
            } else {
                let fields: Array = fields.iter().map(|(k, v)| -> Result<Array, JsValue> {
                    let v = convert_data(v)?;
                    Ok(Array::of2(&JsValue::from_str(k), &v))
                }).collect::<Result<_, _>>()?;
                Object::from_entries(&fields)?.into()
            };
            let obj = Object::new();
            Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str(type_name))?;
            Reflect::set(&obj, &JsValue::from_str("data"), &data)?;
            obj.into()
        },
        Data::GenericArray(arr) => {
            let arr = arr.iter().map(|item| convert_data(item)).collect::<Result<Array, _>>()?;
            arr.into()
        },
        Data::Bool(b) => JsValue::from_bool(*b),
        Data::UInt8(v) => JsValue::from_f64((*v).into()),
        Data::UInt16(v) => JsValue::from_f64((*v).into()),
        Data::UInt32(v) => JsValue::from_f64((*v).into()),
        Data::UInt64(v) => JsValue::from_f64(*v as f64),
        Data::SInt8(v) => JsValue::from_f64((*v).into()),
        Data::SInt16(v) => JsValue::from_f64((*v).into()),
        Data::SInt32(v) => JsValue::from_f64((*v).into()),
        Data::SInt64(v) => JsValue::from_f64(*v as f64),
        Data::Float(v) => JsValue::from_f64((*v).into()),
        Data::Double(v) => JsValue::from_f64((*v).into()),
        Data::Pair(fst, snd) => {
            let fst = convert_data(fst)?;
            let snd = convert_data(snd)?;
            Array::of2(&fst, &snd).into()
        },
        Data::UInt8Array(s) => {
            Uint8Array::from(&**s).into()
        },
        Data::String(s) => {
            std::str::from_utf8(&**s).map(JsValue::from_str).unwrap_or_else(|_| Uint8Array::from(&**s).into())
        },
    })
}
