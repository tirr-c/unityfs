use std::ptr::NonNull;
use std::mem::ManuallyDrop;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use js_sys::{Array, Object, Reflect, Uint8Array};

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
            let fields: Array = fields.iter().map(|(k, v)| -> Result<Array, JsValue> {
                let v = convert_data(v)?;
                Ok(Array::of2(&JsValue::from_str(k), &v))
            }).collect::<Result<_, _>>()?;
            let entries = Object::from_entries(&fields)?;
            let obj = Object::new();
            Reflect::set(&obj, &JsValue::from_str("type"), &JsValue::from_str(type_name))?;
            Reflect::set(&obj, &JsValue::from_str("data"), &entries)?;
            obj.unchecked_into::<JsValue>()
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
