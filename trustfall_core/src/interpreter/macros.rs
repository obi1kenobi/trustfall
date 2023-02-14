#[macro_export]
macro_rules! property_stub {
    ($ctxs:ident, $vertex_variant:path $(| $other_variant:path)*, $vertex:ident, $impl:block) => {
        Box::new($ctxs.map(move |ctx| {
            let value = match &ctx.active_vertex {
                Some($vertex_variant($vertex)) => $impl,
                $( Some($other_variant($vertex)) => $impl, )*
                None => FieldValue::Null,

                // If there's only one vertex variant, the below pattern is unreachable.
                // We don't want to cause a lint for the user of the macro, so we suppress it.
                #[allow(unreachable_patterns)]
                Some(x) => {
                    unreachable!(
                        "Unexpected vertex variant encountered! Expecting {:?} but got {:?}",
                        stringify!($vertex_variant $(| $other_variant)*),
                        x
                    );
                }
            };
            (ctx, value)
        }))
    };
}

#[macro_export]
macro_rules! property_group {
    // initial case
    ($ctxs:ident, $field_name:ident, $vertex_variant:path $(| $other_variant:path)*,
        [
            $($rest:tt),+ $(,)?
        ] $(,)?
    ) => {
        $crate::property_group!( @( $ctxs; $field_name; $vertex_variant $(| $other_variant)*; $($rest)+ ), )
    };

    // property name in schema matches field name on vertex variant inner type
    (@($ctxs:ident; $field_name:ident; $vertex_variant:path $(| $other_variant:path)*;
        $prop_and_field:ident $($rest:tt)*
    ), $($arms:tt)*) => {
        $crate::property_group!(
            @($ctxs; $field_name; $vertex_variant $(| $other_variant)*; $($rest)*),
            $($arms)*
            stringify!($prop_and_field) => {
                $crate::property_stub!($ctxs, $vertex_variant $(| $other_variant)*, vertex, {
                    vertex.$prop_and_field.clone().into()
                })
            }
        )
    };

    // (property name, field name on vertex variant inner type)
    (@($ctxs:ident; $field_name:ident; $vertex_variant:path $(| $other_variant:path)*;
        ($prop_name:ident, $field:ident $(,)? ) $($rest:tt)*
    ), $($arms:tt)*) => {
        $crate::property_group!(
            @($ctxs; $field_name; $vertex_variant $(| $other_variant:path)*; $($rest)*),
            $($arms)*
            stringify!($prop_name) => {
                $crate::property_stub!($ctxs, $vertex_variant $(| $other_variant)*, vertex, {
                    vertex.$field.clone().into()
                })
            }
        )
    };

    // (property name, destructured vertex variant, handler block)
    (@($ctxs:ident; $field_name:ident; $vertex_variant:path $(| $other_variant:path)*;
        ($prop_name:ident, $vertex:ident, $impl:block $(,)? ) $($rest:tt)*
    ), $($arms:tt)*) => {
        $crate::property_group!(
            @($ctxs; $field_name; $vertex_variant $(| $other_variant)*; $($rest)*),
            $($arms)*
            stringify!($prop_name) => {
                $crate::property_stub!($ctxs, $vertex_variant $(| $other_variant)*, $vertex, $impl)
            }
        )
    };

    // final case
    (@($ctxs:ident; $field_name:ident; $vertex_variant:path $(| $other_variant:path)*; ),
        $($arms:tt)+
    ) => {
        match $field_name.as_ref() {
            $($arms)+
            _ => unreachable!(
                "Unexpected property name {} for vertex variant {:?}",
                $field_name.as_ref(), stringify!($vertex_variant $(| $other_variant:path)*)
            ),
        }
    }
}

#[macro_export]
macro_rules! resolve_property {
    ($ctxs:ident, $type_name:ident, $field_name:ident,
        [
            $(
                {
                    $type_option:ident $(| $other_type_option:ident)*,
                    $vertex_variant:path $(| $other_variant:path)*,
                    [ $($rest:tt),+ $(,)? ] $(,)?
                }
            ),+ $(,)?
        ] $(,)?
    ) => {
        match $type_name.as_ref() {
            $(
                stringify!($type_option) $(| stringify!($other_type_option))* => {
                    $crate::property_group!($ctxs, $field_name, $vertex_variant $(| $other_variant)*, [ $($rest),+ ])
                }
            )+
            _ => unreachable!(
                "Unexpected type name {}", $type_name.as_ref()
            ),
        }
    };
}

#[macro_export]
macro_rules! neighbor_stub {
    ($ctxs:ident, $lt:lifetime, $vertex_variant:path $(| $other_variant:path)*, $vertex:ident, $impl:tt) => {
        Box::new($ctxs.map(move |ctx| {
            let neighbors: VertexIterator<$lt, <Self as Adapter>::Vertex>> =
                match &ctx.active_vertex {
                    Some($vertex_variant($vertex)) => $impl,
                    $( Some($other_variant($vertex)) => $impl, )*
                    None => Box::new(std::iter::empty()),

                    // If there's only one vertex variant, the below pattern is unreachable.
                    // We don't want to cause a lint for the user of the macro, so we suppress it.
                    #[allow(unreachable_patterns)]
                    Some(x) => {
                        unreachable!(
                            "Unexpected vertex variant encountered! Expecting {} but got {:?}",
                            stringify!($vertex_variant $(| $other_variant)*),
                            x
                        );
                    }
                };
            (ctx, neighbors)
        }))
    };
}

#[macro_export]
macro_rules! neighbor_group {
    // initial case
    ($ctxs:ident, $lt:lifetime, $edge_name_var:ident, $vertex_variant:path $(| $other_variant:path)*,
        [
            $($rest:tt),+ $(,)?
        ] $(,)?
    ) => {
        $crate::neighbor_group!( @( $ctxs; $lt; $edge_name_var; $vertex_variant $(| $other_variant)*; $($rest)+ ), )
    };

    // edge name in schema matches field name on vertex variant inner type,
    // so matching (edge_and_field, resulting vertex variant)
    (@($ctxs:ident; $lt:lifetime; $edge_name_var:ident; $vertex_variant:path $(| $other_variant:path)*;
        (
            $edge_and_field:ident,
            $next_variant:path $(,)?
        ) $($rest:tt)*
    ), $($arms:tt)*) => {
        $crate::neighbor_group!(
            @($ctxs; $lt; $edge_name_var; $vertex_variant $(| $other_variant)*; $($rest)*),
            $($arms)*
            stringify!($edge_and_field) => {
                $crate::neighbor_stub!($ctxs, $lt, $vertex_variant, vertex, {
                    Box::new(vertex.$edge_and_field.iter().map($next_variant))
                })
            }
        )
    };

    // (edge name, field name on vertex variant inner type, resulting vertex variant)
    (@($ctxs:ident; $lt:lifetime; $edge_name_var:ident; $vertex_variant:path $(| $other_variant:path)*;
        (
            $edge_name:ident,
            $field:ident,
            $next_variant:path $(,)?
        ) $($rest:tt)*
    ), $($arms:tt)*) => {
        $crate::neighbor_group!(
            @($ctxs; $lt; $edge_name_var; $vertex_variant $(| $other_variant)*; $($rest)*),
            $($arms)*
            stringify!($edge_name) => {
                $crate::neighbor_stub!($ctxs, $lt, $vertex_variant $(| $other_variant)*, vertex, {
                    Box::new(vertex.$field.iter().map($next_variant))
                })
            }
        )
    };

    // (edge name, destructured vertex variant, handler block)
    (@($ctxs:ident; $lt:lifetime; $edge_name_var:ident; $vertex_variant:path $(| $other_variant:path)*;
        (
            $edge_name:ident,
            $vertex:ident,
            $impl:block $(,)?
        ) $($rest:tt)*
    ), $($arms:tt)*) => {
        $crate::neighbor_group!(
            @($ctxs; $lt; $edge_name_var; $vertex_variant $(| $other_variant)*; $($rest)*),
            $($arms)*
            stringify!($edge_name) => {
                $crate::neighbor_stub!($ctxs, $lt, $vertex_variant $(| $other_variant)*, $vertex, $impl)
            }
        )
    };

    // final case
    (@($ctxs:ident; $lt:lifetime; $edge_name_var:ident; $vertex_variant:path $(| $other_variant:path)*; ),
        $($arms:tt)+
    ) => {
        match $edge_name_var.as_ref() {
            $($arms)+
            _ => unreachable!(
                "Unexpected edge name {} for vertex variant {:?}",
                $edge_name_var.as_ref(), stringify!($vertex_variant $(| $other_variant)*)
            ),
        }
    }
}

#[macro_export]
macro_rules! resolve_neighbors {
    ($ctxs:ident, $lt:lifetime, $type_name_var:ident, $edge_name_var:ident,
        [
            $(
                {
                    $type_option:ident $(| $other_type_option:ident)*,
                    $vertex_variant:path $(| $other_variant:path)*,
                    [ $($rest:tt),+ $(,)? ] $(,)?
                }
            ),+ $(,)?
        ] $(,)?
    ) => {
        match $type_name_var.as_ref() {
            $(
                stringify!($type_option) $(| stringify!($other_type_option))* => {
                    $crate::neighbor_group!($ctxs, $lt, $edge_name_var, $vertex_variant $(| $other_variant)*, [ $($rest),+ ])
                }
            )+
            _ => unreachable!(
                "Unexpected type name {}", $type_name_var.as_ref()
            ),
        }
    };
}

#[macro_export]
macro_rules! resolve_neighbors2_match_arm {
    // vertex field is same as edge name, so this is just reading the next variant
    ($ctxs:ident, $lt:lifetime, $edge_name:ident, $vertex_variant:path $(| $other_variant:path)*, $next_variant:path $(,)?) => {
        $crate::neighbor_stub!(
            $ctxs, $lt, $vertex_variant $(| $other_variant)*, vertex, {
                Box::new(vertex.$edge_name.iter().map($next_variant))
            }
        )
    };

    // (vertex field, next variant)
    ($ctxs:ident, $lt:lifetime, $edge_name:ident, $vertex_variant:path $(| $other_variant:path)*, ($field:ident, $next_variant:path $(,)?) $(,)?) => {
        $crate::neighbor_stub!(
            $ctxs, $lt, $vertex_variant $(| $other_variant)*, vertex, {
                Box::new(vertex.$field.iter().map($next_variant))
            }
        )
    };

    // (destuctured vertex var, impl block)
    ($ctxs:ident, $lt:lifetime, $edge_name:ident, $vertex_variant:path $(| $other_variant:path)*, ($vertex:ident, $impl:block $(,)?) $(,)?) => {
        $crate::neighbor_stub!($ctxs, $lt, $vertex_variant $(| $other_variant)*, $vertex, $impl)
    };
}

// TODO: figure out whether we want both this macro and the other neighbors macro, or just one
#[macro_export]
macro_rules! resolve_neighbors2 {
    ($ctxs:ident, $lt:lifetime, $type_name_var:ident, $edge_name_var:ident,
        [
            $(
                {
                    $type_option:ident $(| $other_type_option:ident)*,
                    $edge_name:ident,
                    $vertex_variant:path $(| $other_variant:path)*,
                    $rest:tt $(,)?
                }
            ),+ $(,)?
        ] $(,)?
    ) => {
        match ($edge_name_var.as_ref(), $type_name_var.as_ref()) {
            $(
                (stringify!($edge_name), stringify!($type_option)) $(| (stringify!($edge_name), stringify!($other_type_option)))* => {
                    $crate::resolve_neighbors2_match_arm!($ctxs, $lt, $edge_name, $vertex_variant $(| $other_variant)*, $rest )
                }
            )+
            _ => unreachable!(
                "Unexpected type and edge name {} {}", $type_name_var.as_ref(), $edge_name_var.as_ref(),
            ),
        }
    };
}
