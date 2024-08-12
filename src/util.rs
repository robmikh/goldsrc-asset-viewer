#[derive(Debug, PartialEq, Eq)]
pub enum BasicEnumParseError<T> {
    InvalidStr,
    InvalidValue(T),
}

#[macro_export]
macro_rules! basic_enum {
    ($name:ident : $ty:ty { $($var_name:ident = $var_value:expr),*$(,)* } ) => {
        #[repr(i32)]
        #[derive(Copy, Clone, PartialEq, Eq)]
        pub enum $name {
            $(
                $var_name = $var_value,
            )*
        }

        impl $name {
            pub fn from_value(value: $ty) -> Option<Self> {
                match value {
                    $(
                        $var_value => Some(Self::$var_name),
                    )*
                    _ => None,
                }
            }
        }

        impl std::str::FromStr for $name {
            type Err = crate::util::BasicEnumParseError<$ty>;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let value: $ty = s.parse().map_err(|_| Self::Err::InvalidStr)?;
                Self::from_value(value).ok_or(Self::Err::InvalidValue(value))
            }
        }
    };
}
