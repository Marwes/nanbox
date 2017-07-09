#[macro_use]
extern crate nanbox;

make_nanbox!{
    pub unsafe enum Value, Variant {
        Float(f64),
        Byte(u8),
        Int(i32),
        Pointer(*mut Value)
    }
}
