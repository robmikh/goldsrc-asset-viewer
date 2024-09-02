use std::collections::HashMap;

use crate::rendering::bsp::RenderMode;

pub struct TargetName(pub String);
pub enum ModelReference {
    Index(usize),
    Path(String),
}

pub trait ParseEntityValue {
    fn parse(name: &str, values: &HashMap<&str, &str>) -> Self;
}

impl ParseEntityValue for TargetName {
    fn parse(name: &str, values: &HashMap<&str, &str>) -> Self {
        let value = values.get(name).unwrap().to_string();
        Self(value)
    }
}

impl ParseEntityValue for String {
    fn parse(name: &str, values: &HashMap<&str, &str>) -> Self {
        let value = values.get(name).unwrap().to_string();
        value
    }
}

impl ParseEntityValue for ModelReference {
    fn parse(name: &str, values: &HashMap<&str, &str>) -> Self {
        let str_value = values.get(name).unwrap();
        if str_value.starts_with("*") {
            let model_index: usize = str_value.trim_start_matches('*').parse().unwrap();
            Self::Index(model_index)
        } else {
            Self::Path(str_value.to_string())
        }
    }
}

impl ParseEntityValue for [i32; 3] {
    fn parse(name: &str, values: &HashMap<&str, &str>) -> Self {
        let str_value = values.get(name).unwrap();
        let mut parts = str_value.split_whitespace();
        let x: i32 = parts.next().unwrap().parse().unwrap();
        let y: i32 = parts.next().unwrap().parse().unwrap();
        let z: i32 = parts.next().unwrap().parse().unwrap();
        assert_eq!(parts.next(), None);
        [x, y, z]
    }
}

impl ParseEntityValue for i32 {
    fn parse(name: &str, values: &HashMap<&str, &str>) -> Self {
        let str_value = values.get(name).unwrap();
        let value: i32 = str_value.parse().unwrap();
        value
    }
}

impl<T: ParseEntityValue> ParseEntityValue for Option<T> {
    fn parse(name: &str, values: &HashMap<&str, &str>) -> Self {
        if values.contains_key(name) {
            let value = T::parse(name, values);
            Some(value)
        } else {
            None
        }
    }
}

pub trait ParseEntity {
    fn parse(values: &HashMap<&str, &str>) -> Self;
}

macro_rules! parse_entity_struct {
    ($entity_name:ident { }) => {
        pub struct $entity_name {}

        impl ParseEntity for $entity_name {
            fn parse(_values: &HashMap<&str, &str>) -> Self {
                Self {}
            }
        }
    };

    ($entity_name:ident { $( ($key_name:literal) $field_name:ident : $field_ty:ty),* $(,)* }) => {
        pub struct $entity_name {
            $(
                pub $field_name : $field_ty,
            )*
        }

        impl ParseEntity for $entity_name {
            fn parse(values: &HashMap<&str, &str>) -> Self {
                $(
                    let $field_name = < $field_ty >::parse($key_name, values);
                )*
                Self {
                    $(
                        $field_name,
                    )*
                }
            }
        }
    };

    ($entity_name:ident { $( ($key_name:literal) $field_name:ident : $field_ty:ty,)* $ex_name:ident : $ex_ty:ty $(,)* }) => {
        pub struct $entity_name {
            $(
                pub $field_name : $field_ty,
            )*
            pub $ex_name: $ex_ty,
        }

        impl ParseEntity for $entity_name {
            fn parse(values: &HashMap<&str, &str>) -> Self {
                $(
                    let $field_name = < $field_ty >::parse($key_name, values);
                )*
                let $ex_name = < $ex_ty >::parse(values);
                Self {
                    $(
                        $field_name,
                    )*
                    $ex_name,
                }
            }
        }
    };
}

parse_entity_struct!(Entity {
    ("targetname") name : Option<String>,
    ("parentname") parent: Option<TargetName>,
    ("classname") class_name: String,
    ("model") model: Option<ModelReference>,
    ("origin") origin: Option<[i32; 3]>,
    ("angles") angles: Option<[i32; 3]>,
    ("spawnflags") spawn_flags: Option<i32>,

    // TODO: What to do with common properties?
    ("rendermode") render_mode: Option<RenderMode>,
    ("renderamt") render_amount: Option<i32>,
    ("angle") angle: Option<i32>, // https://developer.valvesoftware.com/wiki/Info_player_start_(GoldSrc) says info_player_start has angles but c1a0 uses angle

    ex: EntityEx,
});

macro_rules! parse_entity_enum {
    ($enum_name:ident { $(($var_class_name:literal)  $var_name:ident($var_ty:ty)),* $(,)* }) => {
        pub enum $enum_name {
            $(
                $var_name($var_ty),
            )*
            Unknown(UnknownEntityValues),
        }

        impl ParseEntity for $enum_name {
            fn parse(values: &HashMap<&str, &str>) -> Self {
                let class_name = values.get("classname").unwrap();
                match *class_name {
                    $(
                        $var_class_name => {
                            let value = <$var_ty>::parse(values);
                            Self::$var_name(value)
                        },
                    )*
                    _ => {
                        let value = <UnknownEntityValues>::parse(values);
                        Self::Unknown(value)
                    }
                }
            }
        }
    }
}

pub struct UnknownEntityValues(pub HashMap<String, String>);

impl ParseEntity for UnknownEntityValues {
    fn parse(values: &HashMap<&str, &str>) -> Self {
        let mut new_values = HashMap::with_capacity(values.len());
        for (key, value) in values {
            new_values.insert(key.to_string(), value.to_string());
        }
        Self(new_values)
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
    ("angle") angle: i32,
    ("lip") lip: i32,
    ("speed") speed: i32,
});
