use std::fmt;
use std::mem;

const DOUBLE_MAX_TAG: u32 = 0x1FFF0;
const SHIFTED_DOUBLE_MAX_TAG: u64 = ((DOUBLE_MAX_TAG as u64) << 47) | 0xFFFFFFFF;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct NanBox(u64);

impl fmt::Debug for NanBox {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
               "NanBox {{ tag: {:?}, payload: {:?} }}",
               self.tag(),
               self.0 & ((1 << 48) - 1))
    }
}

pub trait NanBoxable: Sized {
    unsafe fn from_nan_box(n: NanBox) -> Self;

    fn into_nan_box(self) -> NanBox;

    fn pack_nan_box(self, tag: u8) -> NanBox {
        let mut b = self.into_nan_box();

        let shifted_tag = ((DOUBLE_MAX_TAG as u64) | (tag as u64)) << 47;
        b.0 |= shifted_tag;
        debug_assert!(b.tag() == u32::from(tag), "{} == {}", b.tag(), tag);
        b
    }

    unsafe fn unpack_nan_box(value: NanBox) -> Self {
        let mask = (1 << 48) - 1;
        let b = NanBox(value.0 & mask);
        Self::from_nan_box(b)
    }
}

impl NanBoxable for f64 {
    unsafe fn from_nan_box(n: NanBox) -> f64 {
        mem::transmute(n)
    }

    fn into_nan_box(self) -> NanBox {
        unsafe { NanBox(mem::transmute(self)) }
    }

    fn pack_nan_box(self, tag: u8) -> NanBox {
        debug_assert!(tag == 0);
        self.into_nan_box()
    }

    unsafe fn unpack_nan_box(value: NanBox) -> Self {
        Self::from_nan_box(value)
    }
}

macro_rules! impl_cast {
    ($($typ: ident)+) => {
        $(
        impl NanBoxable for $typ {
            unsafe fn from_nan_box(n: NanBox) -> $typ {
                n.0 as $typ
            }

            fn into_nan_box(self) -> NanBox {
                NanBox(self as u64)
            }
        }
        )*
    }
}

impl_cast!{ u8 u16 u32 i8 i16 i32 }

macro_rules! impl_cast_t {
    ($param: ident, $($typ: ty)+) => {
        $(
        impl<$param> NanBoxable for $typ {
            unsafe fn from_nan_box(n: NanBox) -> $typ {
                n.0 as $typ
            }

            fn into_nan_box(self) -> NanBox {
                debug_assert!((self as u64) >> 48 == 0);
                NanBox(self as u64)
            }
        }
        )*
    }
}

impl_cast_t! { T, *mut T *const T }

impl NanBox {
    pub unsafe fn new<T>(tag: u8, value: T) -> NanBox
        where T: NanBoxable
    {
        value.pack_nan_box(tag)
    }

    pub unsafe fn unpack<T>(self) -> T
        where T: NanBoxable
    {
        T::unpack_nan_box(self)
    }

    pub fn tag(self) -> u32 {
        if self.0 <= SHIFTED_DOUBLE_MAX_TAG {
            0
        } else {
            (self.0 >> 47) as u32 & !DOUBLE_MAX_TAG
        }
    }
}

macro_rules! make_nanbox {
    (
        $(#[$meta:meta])*
        pub enum $name: ident, $enum_name: ident {
            $($field: ident ($typ: ty)),*
        }
    ) => {
        
        $(#[$meta])*
        pub struct $name {
            _marker: ::std::marker::PhantomData<($($typ),*)>,
            value: $crate::NanBox,
        }

        $(#[$meta])*
        pub enum $enum_name {
            $(
                $field($typ),
            )+
        }

        $(
            impl From<$typ> for $name {
                fn from(value: $typ) -> $name {
                    $name::from($enum_name::$field(value))
                }
            }
        )+

        impl From<$enum_name> for $name {
            fn from(value: $enum_name) -> $name {
                #[allow(unused_assignments)]
                unsafe {
                    let mut tag = 0;
                    $(
                        if let $enum_name::$field(value) = value {
                            return $name {
                                _marker: ::std::marker::PhantomData,
                                value: $crate::NanBox::new(tag, value)
                            };
                        }
                        tag += 1;
                    )+
                    unreachable!()
                }
            }
        }

        impl $name {
            pub fn into_variant(self) -> $enum_name {
                #[allow(unused_assignments)]
                unsafe {
                    let mut expected_tag = 0;
                    $(
                        if expected_tag == self.value.tag() {
                            return $enum_name::$field(self.value.unpack());
                        }
                        expected_tag += 1;
                    )*
                    debug_assert!(false, "Unexpected tag {}", self.value.tag());
                    unreachable!()
                }
            }
        }
    }
}

#[cfg(test)]
#[macro_use]
extern crate quickcheck;

#[cfg(test)]
mod tests {
    use super::*;

    use std::f64;
    use std::fmt;

    use quickcheck::TestResult;

    fn test_eq<T>(l: T, r: T) -> TestResult
        where T: PartialEq + fmt::Debug
    {
        if l == r {
            TestResult::passed()
        } else {
            TestResult::error(format!("{:?} != {:?}", l, r))
        }
    }

    quickcheck!{
        fn nanbox_f64(f: f64) -> TestResult {
            unsafe {
                test_eq(NanBox::new(0, f).unpack(), f)
            }
        }

        fn nanbox_u32(tag: u8, v: u32) -> TestResult {
            if tag == 0 || tag >= 8 {
                return TestResult::discard();
            }
            unsafe {
                TestResult::from_bool(NanBox::new(tag, v).tag() == tag as u32)
            }
        }
    }

    make_nanbox!{
        #[derive(Debug, PartialEq)]
        pub enum Value, Variant {
            Float(f64),
            Int(i32),
            Pointer(*mut ())
        }
    }

    #[test]
    fn box_test() {
        assert_eq!(Value::from(123).into_variant(), Variant::Int(123));
        assert_eq!(Value::from(3000 as *mut ()).into_variant(),
                   Variant::Pointer(3000 as *mut ()));
        assert_eq!(Value::from(3.14).into_variant(), Variant::Float(3.14));
    }

    #[test]
    fn nan_box_nan() {
        match Value::from(f64::NAN).into_variant() {
            Variant::Float(x) => assert!(x.is_nan()),
            x => panic!("Unexpected {:?}", x),
        }
    }

    #[should_panic]
    #[test]
    fn invalid_pointer() {
        ((1u64 << 48) as *const ()).into_nan_box();
    }
}
