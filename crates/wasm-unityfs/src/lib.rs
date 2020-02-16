use std::ptr::NonNull;
use std::mem::ManuallyDrop;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use js_sys::{Array, Object, Reflect, Uint8Array};
use web_sys::ImageData;

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
    pub fn load(input: Vec<u8>) -> Self {
        let input = unsafe { NonNull::new_unchecked(Box::into_raw(input.into_boxed_slice())) };
        let input_ref: &'static [u8] = unsafe { &*input.as_ptr() };
        let (_, meta) = unityfs::UnityFsMeta::parse(input_ref).unwrap_throw();
        let meta = unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(meta))) };
        let meta_ref: &'static _ = unsafe { &*meta.as_ptr() };
        let fs = meta_ref.read_unityfs();
        Self {
            input,
            meta,
            fs: ManuallyDrop::new(fs),
        }
    }

    pub fn asset_objects(&self) -> Result<Array, JsValue> {
        self.fs.assets().iter().map(|asset| {
            asset.objects().iter().map(|obj| convert_data(&obj.data)).collect::<Result<Array, _>>()
        }).collect()
    }
}

#[wasm_bindgen]
pub struct Asset {
}

fn read_image_data(mut image_data: impl std::io::Read, format: etcdec::DecodeFormat, width: u32, height: u32) -> Result<ImageData, JsValue> {
    let block_width = (width + 3) / 4;
    let block_height = (height + 3) / 4;
    let scanline = (width * 4) as usize;
    let mut buf = vec![0u8; scanline * height as usize];
    for block_y in 0..block_height {
        let y = block_y * 4;
        for block_x in 0..block_width {
            let x = block_x * 4;
            let block = etcdec::decode_single_block(&mut image_data, format).map_err(|_| JsValue::from_str("read error"))?;
            for (block_raw, target) in block.iter().zip(buf[(4 * x as usize)..].chunks_mut(scanline).rev().skip(y as usize).take(4)) {
                target[..16].copy_from_slice(block_raw);
            }
        }
    }
    ImageData::new_with_u8_clamped_array(wasm_bindgen::Clamped(&mut buf), width)
}

fn convert_data(data: &unityfs::Data<'_>) -> Result<JsValue, JsValue> {
    use unityfs::Data;

    let ret = match data {
        Data::GenericPrimitive { type_name, data } => {
            let obj = Object::new();
            Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str(type_name))?;
            Reflect::set(&obj, &JsValue::from_str("data"), &Uint8Array::from(*data))?;
            obj.unchecked_into::<JsValue>()
        },
        Data::GenericStruct { type_name, fields } => {
            if type_name == "Texture2D" {
                let format = match fields.get("m_TextureFormat") {
                    Some(Data::SInt32(34)) => etcdec::DecodeFormat::EtcRgb4,
                    Some(Data::SInt32(45)) => etcdec::DecodeFormat::Etc2Rgb,
                    Some(Data::SInt32(46)) => etcdec::DecodeFormat::Etc2Rgba1,
                    Some(Data::SInt32(47)) => etcdec::DecodeFormat::Etc2Rgba8,
                    Some(Data::SInt32(_)) => return Err(JsValue::from_str("unknown texture format")),
                    _ => return Err(JsValue::from_str("m_TextureFormat type mismatch")),
                };
                let width = if let Some(Data::SInt32(width)) = fields.get("m_Width") {
                    *width
                } else {
                    return Err(JsValue::from_str("m_Width type mismatch"));
                };
                let height = if let Some(Data::SInt32(height)) = fields.get("m_Height") {
                    *height
                } else {
                    return Err(JsValue::from_str("m_Width type mismatch"));
                };
                let image_data: &[u8] = if let Some(Data::UInt8Array(buf)) = fields.get("image data") {
                    buf
                } else {
                    return Err(JsValue::from_str("m_Width type mismatch"));
                };
                let image_data = std::io::Cursor::new(image_data);
                let image_data = read_image_data(image_data, format, width as u32, height as u32)?.unchecked_into::<JsValue>();

                let data = Object::new();
                let name = fields.get("m_Name").ok_or_else(|| JsValue::from_str("m_Name expected"))?;
                let name = convert_data(name)?;
                Reflect::set(&data, &JsValue::from_str("m_Name"), &name)?;
                Reflect::set(&data, &JsValue::from_str("image"), &image_data)?;
                let obj = Object::new();
                Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str(type_name))?;
                Reflect::set(&obj, &JsValue::from_str("data"), &data)?;
                obj.unchecked_into::<JsValue>()
            } else {
                let fields: Array = fields.iter().map(|(k, v)| -> Result<Array, JsValue> {
                    let v = convert_data(v)?;
                    Ok(Array::of2(&JsValue::from_str(k), &v))
                }).collect::<Result<_, _>>()?;
                let entries = Object::from_entries(&fields)?;
                let obj = Object::new();
                Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str(type_name))?;
                Reflect::set(&obj, &JsValue::from_str("data"), &entries)?;
                obj.unchecked_into::<JsValue>()
            }
        },
        Data::GenericArray(arr) => {
            let arr = arr.iter().map(|item| convert_data(item)).collect::<Result<Array, _>>()?;
            arr.unchecked_into::<JsValue>()
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
            Array::of2(&fst, &snd).unchecked_into::<JsValue>()
        },
        Data::UInt8Array(s) => {
            Uint8Array::from(*s).unchecked_into::<JsValue>()
        },
        Data::String(s) => {
            std::str::from_utf8(*s).map(JsValue::from_str).unwrap_or_else(|_| Uint8Array::from(*s).unchecked_into::<JsValue>())
        },
    };
    Ok(ret)
}
