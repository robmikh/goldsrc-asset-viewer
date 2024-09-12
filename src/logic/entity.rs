use std::collections::HashMap;

use glam::Vec3;

use crate::rendering::bsp::RenderMode;

pub struct TargetName(pub String);
#[allow(dead_code)]
pub enum ModelReference {
    Index(usize),
    Path(String),
}

#[derive(Copy, Clone, Debug)]
pub enum EntityParseError<'a> {
    InvalidValue {
        value_name: &'a str,
        value_str: &'a str,
    },
    MissingValue {
        value_name: &'a str,
    },
}

impl<'a> std::fmt::Display for EntityParseError<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityParseError::InvalidValue {
                value_name,
                value_str,
            } => write!(
                f,
                "InvalidValue{{ name: {}, value: {} }}",
                value_name, value_str
            ),
            EntityParseError::MissingValue { value_name } => {
                write!(f, "MissingValue{{ name: {} }}", value_name)
            }
        }
    }
}

pub type ParseEntityResult<'a, T> = std::result::Result<T, EntityParseError<'a>>;

pub trait OkOr<T>: Sized {
    fn ok_or<E>(self, err: E) -> std::result::Result<T, E>;
}

impl<T: std::str::FromStr> OkOr<T> for std::result::Result<T, T::Err> {
    fn ok_or<E>(self, err: E) -> std::result::Result<T, E> {
        match self {
            Ok(value) => Ok(value),
            Err(_) => Err(err),
        }
    }
}

pub trait ParseEntityValue: Sized {
    fn parse<'a>(
        name: &'a str,
        values: &'a HashMap<&'a str, &'a str>,
    ) -> ParseEntityResult<'a, Self>;
}

impl ParseEntityValue for TargetName {
    fn parse<'a>(
        name: &'a str,
        values: &'a HashMap<&'a str, &'a str>,
    ) -> ParseEntityResult<'a, Self> {
        if let Some(value) = values.get(name) {
            Ok(Self(value.to_string()))
        } else {
            Err(EntityParseError::MissingValue { value_name: name })
        }
    }
}

impl ParseEntityValue for String {
    fn parse<'a>(
        name: &'a str,
        values: &'a HashMap<&'a str, &'a str>,
    ) -> ParseEntityResult<'a, Self> {
        if let Some(value) = values.get(name) {
            Ok(value.to_string())
        } else {
            Err(EntityParseError::MissingValue { value_name: name })
        }
    }
}

impl ParseEntityValue for ModelReference {
    fn parse<'a>(
        name: &'a str,
        values: &'a HashMap<&'a str, &'a str>,
    ) -> ParseEntityResult<'a, Self> {
        if let Some(str_value) = values.get(name) {
            if str_value.starts_with("*") {
                let str_value = str_value.trim_start_matches('*');
                if let Ok(model_index) = str_value.parse::<usize>() {
                    Ok(Self::Index(model_index))
                } else {
                    Err(EntityParseError::InvalidValue {
                        value_name: name,
                        value_str: str_value,
                    })
                }
            } else {
                Ok(Self::Path(str_value.to_string()))
            }
        } else {
            Err(EntityParseError::MissingValue { value_name: name })
        }
    }
}

impl ParseEntityValue for [i32; 3] {
    fn parse<'a>(
        name: &'a str,
        values: &'a HashMap<&'a str, &'a str>,
    ) -> ParseEntityResult<'a, Self> {
        if let Some(str_value) = values.get(name) {
            let invalid_value_err = EntityParseError::InvalidValue {
                value_name: name,
                value_str: str_value,
            };

            let mut parts = str_value.split_whitespace();
            let x: i32 = parts
                .next()
                .ok_or(invalid_value_err)?
                .parse()
                .ok_or(invalid_value_err)?;

            // Special case a single zero as all zeroes
            let next = parts.next();
            if x == 0 && next.is_none() {
                return Ok([0, 0, 0]);
            }

            let y: i32 = next
                .ok_or(invalid_value_err)?
                .parse()
                .ok_or(invalid_value_err)?;
            let z: i32 = parts
                .next()
                .ok_or(invalid_value_err)?
                .parse()
                .ok_or(invalid_value_err)?;
            assert_eq!(parts.next(), None);
            Ok([x, y, z])
        } else {
            Err(EntityParseError::MissingValue { value_name: name })
        }
    }
}

impl ParseEntityValue for i32 {
    fn parse<'a>(
        name: &'a str,
        values: &'a HashMap<&'a str, &'a str>,
    ) -> ParseEntityResult<'a, Self> {
        if let Some(str_value) = values.get(name) {
            let invalid_value_err = EntityParseError::InvalidValue {
                value_name: name,
                value_str: str_value,
            };

            let value: i32 = str_value.parse().ok_or(invalid_value_err)?;
            Ok(value)
        } else {
            Err(EntityParseError::MissingValue { value_name: name })
        }
    }
}

impl ParseEntityValue for [f32; 3] {
    fn parse<'a>(
        name: &'a str,
        values: &'a HashMap<&'a str, &'a str>,
    ) -> ParseEntityResult<'a, Self> {
        if let Some(str_value) = values.get(name) {
            let invalid_value_err = EntityParseError::InvalidValue {
                value_name: name,
                value_str: str_value,
            };

            let mut parts = str_value.split_whitespace();
            let x: f32 = parts
                .next()
                .ok_or(invalid_value_err)?
                .parse()
                .ok_or(invalid_value_err)?;

            // Special case a single zero as all zeroes
            let next = parts.next();
            if x == 0.0 && next.is_none() {
                return Ok([0.0, 0.0, 0.0]);
            }

            let y: f32 = next
                .ok_or(invalid_value_err)?
                .parse()
                .ok_or(invalid_value_err)?;
            let z: f32 = parts
                .next()
                .ok_or(invalid_value_err)?
                .parse()
                .ok_or(invalid_value_err)?;
            assert_eq!(parts.next(), None);
            Ok([x, y, z])
        } else {
            Err(EntityParseError::MissingValue { value_name: name })
        }
    }
}

impl ParseEntityValue for f32 {
    fn parse<'a>(
        name: &'a str,
        values: &'a HashMap<&'a str, &'a str>,
    ) -> ParseEntityResult<'a, Self> {
        if let Some(str_value) = values.get(name) {
            let invalid_value_err = EntityParseError::InvalidValue {
                value_name: name,
                value_str: str_value,
            };

            let value: f32 = str_value.parse().ok_or(invalid_value_err)?;
            Ok(value)
        } else {
            Err(EntityParseError::MissingValue { value_name: name })
        }
    }
}

impl<T: ParseEntityValue> ParseEntityValue for Option<T> {
    fn parse<'a>(
        name: &'a str,
        values: &'a HashMap<&'a str, &'a str>,
    ) -> ParseEntityResult<'a, Option<T>> {
        if values.contains_key(name) {
            let value = T::parse(name, values)?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }
}

pub trait ParseEntity<'a>: Sized {
    fn parse(values: &'a HashMap<&'a str, &'a str>) -> ParseEntityResult<'a, Self>;
}

macro_rules! parse_entity_struct {
    ($entity_name:ident { }) => {
        pub struct $entity_name {}

        impl<'a> ParseEntity<'a> for $entity_name {
            fn parse(_values: &'a HashMap<&'a str, &'a str>) -> ParseEntityResult<'a, Self> {
                Ok(Self {})
            }
        }
    };

    ($entity_name:ident { $( ($key_name:literal) $field_name:ident : $field_ty:ty),* $(,)* }) => {
        #[allow(dead_code)]
        pub struct $entity_name {
            $(
                pub $field_name : $field_ty,
            )*
        }

        impl<'a> ParseEntity<'a> for $entity_name {
            fn parse(values: &'a HashMap<&'a str, &'a str>) -> ParseEntityResult<'a, Self> {
                $(
                    let $field_name = < $field_ty >::parse($key_name, values)?;
                )*
                Ok(Self {
                    $(
                        $field_name,
                    )*
                })
            }
        }
    };

    ($entity_name:ident { $( ($key_name:literal) $field_name:ident : $field_ty:ty,)* $ex_name:ident : $ex_ty:ty $(,)* }) => {
        #[allow(dead_code)]
        pub struct $entity_name {
            $(
                pub $field_name : $field_ty,
            )*
            pub $ex_name: $ex_ty,
        }

        impl<'a> ParseEntity<'a> for $entity_name {
            fn parse(values: &'a HashMap<&'a str, &'a str>) -> ParseEntityResult<'a, Self> {
                $(
                    let $field_name = < $field_ty >::parse($key_name, values)?;
                )*
                let $ex_name = < $ex_ty >::parse(values)?;
                Ok(Self {
                    $(
                        $field_name,
                    )*
                    $ex_name,
                })
            }
        }
    };
}

parse_entity_struct!(Entity {
    ("targetname") name : Option<String>,
    ("parentname") parent: Option<TargetName>,
    ("classname") class_name: String,
    ("model") model: Option<ModelReference>,
    ("origin") origin: Option<[f32; 3]>,
    ("angles") angles: Option<[f32; 3]>,
    ("spawnflags") spawn_flags: Option<i32>,

    // TODO: What to do with common properties?
    ("rendermode") render_mode: Option<RenderMode>,
    ("renderamt") render_amount: Option<i32>,
    ("angle") angle: Option<f32>, // https://developer.valvesoftware.com/wiki/Info_player_start_(GoldSrc) says info_player_start has angles but c1a0 uses angle

    ex: EntityEx,
});

macro_rules! parse_entity_enum {
    ($enum_name:ident { $(($var_class_name:literal)  $var_name:ident($var_ty:ty)),* $(,)* }) => {
        #[allow(dead_code)]
        pub enum $enum_name {
            $(
                $var_name($var_ty),
            )*
            Unknown(UnknownEntityValues),
        }

        impl<'a> ParseEntity<'a> for $enum_name {
            fn parse(values: &'a HashMap<&'a str, &'a str>) -> ParseEntityResult<'a, Self> {
                let class_name = values.get("classname").unwrap();
                match *class_name {
                    $(
                        $var_class_name => {
                            let value = <$var_ty>::parse(values)?;
                            Ok(Self::$var_name(value))
                        },
                    )*
                    _ => {
                        let value = <UnknownEntityValues>::parse(values)?;
                        Ok(Self::Unknown(value))
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
pub struct UnknownEntityValues(pub HashMap<String, String>);

impl<'a> ParseEntity<'a> for UnknownEntityValues {
    fn parse(values: &'a HashMap<&'a str, &'a str>) -> ParseEntityResult<'a, Self> {
        let mut new_values = HashMap::with_capacity(values.len());
        for (key, value) in values {
            new_values.insert(key.to_string(), value.to_string());
        }
        Ok(Self(new_values))
    }
}

parse_entity_enum!(
    EntityEx {
        ("func_wall") FuncWall(FuncWall),
        ("func_door") FuncDoor(FuncDoor),
        ("info_player_start") InfoPlayerStart(InfoPlayerStart),
        ("trigger_changelevel") TriggerChangeLevel(TriggerChangeLevel),
    }
);

parse_entity_struct!(FuncWall {});
parse_entity_struct!(InfoPlayerStart {});
parse_entity_struct!(TriggerChangeLevel {
    ("map") map: String,
    ("landmark") landmark: TargetName,
    ("changetarget") change_target: Option<TargetName>,
});
parse_entity_struct!(FuncDoor {
    ("angle") angle: Option<i32>,
    ("lip") lip: Option<i32>,
    ("speed") speed: Option<f32>,
});

pub enum EntityState {
    None,
    FuncDoor(FuncDoorState),
}

pub struct FuncDoorState {
    pub offset: Vec3,
    pub closed_offset: Vec3,
    pub open_offset: Vec3,
    pub is_open: bool,
}

impl FuncDoor {
    pub fn angle(&self) -> i32 {
        self.angle.unwrap_or(0)
    }
}
