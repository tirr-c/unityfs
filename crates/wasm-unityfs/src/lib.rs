use std::ptr::NonNull;
use std::mem::ManuallyDrop;
use wasm_bindgen::prelude::*;
use js_sys::{Array, Object, Reflect, Uint8Array, Uint8ClampedArray};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
pub struct UnityFs {
    input: NonNull<[u8]>,
    meta: NonNull<unityfs::UnityFsMeta<'static>>,
    fs: ManuallyDrop<unityfs::UnityFs<'static>>,
}

impl Drop for UnityFs {
    fn drop(&mut self) {
        unsafe {
            let input = Box::from_raw(self.input.as_ptr());
            let meta = Box::from_raw(self.meta.as_ptr());
            ManuallyDrop::drop(&mut self.fs);
            drop(meta);
            drop(input);
        }
    }
}

#[wasm_bindgen]
impl UnityFs {
    pub fn load(input: Vec<u8>) -> Result<UnityFs, JsValue> {
        let input = unsafe { NonNull::new_unchecked(Box::into_raw(input.into_boxed_slice())) };
        let input_ref: &'static [u8] = unsafe { &*input.as_ptr() };
        let (_, meta) = unityfs::UnityFsMeta::parse(input_ref).map_err(|e| JsValue::from(format!("parse failed: {:?}", e)))?;
        let meta = unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(meta))) };
        let meta_ref: &'static _ = unsafe { &*meta.as_ptr() };
        let fs = meta_ref.read_unityfs();
        Ok(Self {
            input,
            meta,
            fs: ManuallyDrop::new(fs),
        })
    }

    #[wasm_bindgen(js_name = assetObjects)]
    pub fn asset_objects(&self) -> Result<Array, JsValue> {
        self.fs.assets().iter().map(|asset| {
            asset.objects().iter().map(|obj| convert_data(&obj.data)).collect::<Result<Array, _>>()
        }).collect()
    }
}

#[wasm_bindgen]
pub struct Texture2D {
    name: String,
    #[wasm_bindgen(readonly)]
    pub width: u32,
    #[wasm_bindgen(readonly)]
    pub height: u32,
    image_data: Vec<u8>,
}

impl Texture2D {
    fn read(
        name: String,
        width: u32,
        height: u32,
        format: etcdec::DecodeFormat,
        mut image_data: impl std::io::Read,
    ) -> Result<Self, JsValue> {
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
        Ok(Self {
            name,
            width,
            height,
            image_data: buf,
        })
    }
}

#[wasm_bindgen]
impl Texture2D {
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn image_rgba8(&self) -> Uint8ClampedArray {
        (&*self.image_data).into()
    }

    pub fn encode(&self) -> Result<Vec<u8>, JsValue> {
        let mut buf = Vec::new();
        let w = std::io::BufWriter::new(&mut buf);
        let mut encoder = png::Encoder::new(w, self.width, self.height);
        encoder.set_color(png::ColorType::RGBA);
        encoder.set_depth(png::BitDepth::Eight);
        let mut w = encoder.write_header().map_err(|e| JsValue::from(format!("error initializing encoder: {}", e)))?;
        w.write_image_data(&self.image_data).map_err(|e| JsValue::from(format!("error while encoding: {}", e)))?;
        drop(w);
        Ok(buf)
    }
}

fn convert_data(data: &unityfs::Data<'_>) -> Result<JsValue, JsValue> {
    use unityfs::Data;

    Ok(match data {
        Data::GenericPrimitive { type_name, data } => {
            let obj = Object::new();
            Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str(type_name))?;
            Reflect::set(&obj, &JsValue::from_str("data"), &Uint8Array::from(*data))?;
            obj.into()
        },
        Data::GenericStruct { type_name, fields } => {
            let data = if type_name == "Texture2D" {
                let name = match fields.get("m_Name") {
                    Some(Data::String(s)) => String::from_utf8_lossy(s).into_owned(),
                    Some(_) => return Err("m_Name type mismatch".into()),
                    None => return Err("m_Name not found".into()),
                };
                let format = match fields.get("m_TextureFormat") {
                    Some(Data::SInt32(34)) => etcdec::DecodeFormat::EtcRgb4,
                    Some(Data::SInt32(45)) => etcdec::DecodeFormat::Etc2Rgb,
                    Some(Data::SInt32(46)) => etcdec::DecodeFormat::Etc2Rgba1,
                    Some(Data::SInt32(47)) => etcdec::DecodeFormat::Etc2Rgba8,
                    Some(Data::SInt32(_)) => return Err("unknown texture format".into()),
                    Some(_) => return Err("m_TextureFormat type mismatch".into()),
                    None => return Err("m_TextureFormat not found".into()),
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
                Texture2D::read(name, width, height, format, image_data)?.into()
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
            Uint8Array::from(*s).into()
        },
        Data::String(s) => {
            std::str::from_utf8(*s).map(JsValue::from_str).unwrap_or_else(|_| Uint8Array::from(*s).into())
        },
    })
}
