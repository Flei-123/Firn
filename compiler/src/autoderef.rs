use crate::types::Type;

pub(crate) fn struct_of_ptr(t: &Type) -> Option<usize> {
    match t {
        Type::Ptr { inner, .. } => match **inner {
            Type::Struct(i) => Some(i),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn is_struct_ptr(t: &Type) -> bool {
    struct_of_ptr(t).is_some()
}

pub(crate) fn is_index_int(t: &Type) -> bool {
    t.is_concrete_int()
}
