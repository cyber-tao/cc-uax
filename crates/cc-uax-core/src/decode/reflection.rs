//! The package's own reflected class declarations.
//!
//! A legacy (`FileVersionUE5` < 1012) `TSet`/`TMap` tag records no element struct
//! name, but a generated class's `ChildProperties` do: every Blueprint variable is
//! an `FProperty` whose inner fields name the key and value struct. An export whose
//! class is a reflected class of the same package can therefore be resolved from
//! the package alone, with no table of engine declarations.

use super::DecodedExport;
use crate::property::{ContainerStructNames, ContainerStructs};
use crate::script::field::DecodedField;
use std::collections::HashMap;

/// Superclass chains are short; this only bounds a malformed or cyclic one.
const MAX_SUPER_STRUCT_DEPTH: usize = 64;

/// What one reflected class declares.
struct ReflectedStruct {
    /// The export index of the super struct when it lives in this package. An
    /// import (negative) or none (`0`) ends the walk: what it declares is not
    /// reachable from here.
    super_struct: Option<i32>,
    fields: Vec<DecodedField>,
}

/// The `ChildProperties` of every reflected class export of a package, by
/// `FPackageIndex`.
pub(super) struct PackageReflection {
    structs: HashMap<i32, ReflectedStruct>,
}

impl PackageReflection {
    /// Collects the declarations of the already decoded class exports.
    pub(super) fn from_classes<'a>(classes: impl Iterator<Item = &'a DecodedExport>) -> Self {
        let structs = classes
            .filter_map(|export| {
                let script_struct = export.script_struct.as_ref()?;
                Some((
                    export.identity.index,
                    ReflectedStruct {
                        super_struct: (script_struct.super_struct_index > 0)
                            .then_some(script_struct.super_struct_index),
                        fields: script_struct.properties.clone(),
                    },
                ))
            })
            .collect();
        Self { structs }
    }

    /// The declarations behind an export whose class is `class_index`, when that
    /// class is a reflected class of this package.
    pub(super) fn class_handle(&self, class_index: i32) -> Option<ReflectedClass<'_>> {
        (class_index > 0 && self.structs.contains_key(&class_index)).then_some(ReflectedClass {
            reflection: self,
            class: class_index,
        })
    }
}

/// A reflected class and the declarations it inherits from super classes of this
/// package.
pub(super) struct ReflectedClass<'r> {
    reflection: &'r PackageReflection,
    class: i32,
}

impl ContainerStructNames for ReflectedClass<'_> {
    fn container_struct_names(&self, property: &str, container: &str) -> Option<ContainerStructs> {
        let mut index = self.class;
        for _ in 0..MAX_SUPER_STRUCT_DEPTH {
            let declaration = self.reflection.structs.get(&index)?;
            if let Some(field) = declaration
                .fields
                .iter()
                .find(|field| field.name == property)
            {
                // A property of another kind is not what the tag describes, so
                // nothing here can name its elements.
                if field.type_name != container {
                    return None;
                }
                return Some(ContainerStructs {
                    key: struct_slot(field.inner.first()),
                    value: struct_slot(field.inner.get(1)),
                });
            }
            index = declaration.super_struct?;
        }
        None
    }
}

/// The short struct name a container slot declares: `LinearColor` for
/// `/Script/CoreUObject.LinearColor`, `S_Foo` for `/Game/X/S_Foo.S_Foo`. Only a
/// `StructProperty` that names its struct contributes; an enum-backed byte is left
/// to the exact-fit width probe.
fn struct_slot(field: Option<&DecodedField>) -> Option<String> {
    let field = field?;
    if field.type_name != "StructProperty" {
        return None;
    }
    let full_name = field.type_object.as_deref()?;
    let short = full_name.rsplit(['.', '/']).next()?;
    (!short.is_empty()).then(|| short.to_string())
}
