#[macro_use]
extern crate nanbox;

unsafe_make_nanbox!{
    pub enum Value, Variant {
        Float(f64),
        Byte(u8),
        Int(i32),
        Pointer(*mut Value)
    }
}
