#[macro_export]
macro_rules! property_stub {
    ($ctxs:ident, $token_variant:path $(| $other_variant:path)*, $token:ident, $impl:block) => {
        Box::new($ctxs.map(move |ctx| {
            let value = match &ctx.current_token {
                Some($token_variant($token)) => $impl,
                $( Some($other_variant($token)) => $impl, )*
                None => FieldValue::Null,

                // If there's only one token variant, the below pattern is unreachable.
                // We don't want to cause a lint for the user of the macro, so we suppress it.
                #[allow(unreachable_patterns)]
                Some(x) => {
                    unreachable!(
                        "Unexpected token variant encountered! Expecting {:?} but got {:?}",
                        stringify!($token_variant $(| $other_variant)*),
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
    ($ctxs:ident, $field_name:ident, $token_variant:path $(| $other_variant:path)*,
        [
            $($rest:tt),+ $(,)?
        ] $(,)?
    ) => {
        $crate::property_group!( @( $ctxs; $field_name; $token_variant $(| $other_variant)*; $($rest)+ ), )
    };

    // property name in schema matches field name on token variant inner type
    (@($ctxs:ident; $field_name:ident; $token_variant:path $(| $other_variant:path)*;
        $prop_and_field:ident $($rest:tt)*
    ), $($arms:tt)*) => {
        $crate::property_group!(
            @($ctxs; $field_name; $token_variant $(| $other_variant)*; $($rest)*),
            $($arms)*
            stringify!($prop_and_field) => {
                $crate::property_stub!($ctxs, $token_variant $(| $other_variant)*, token, {
                    token.$prop_and_field.clone().into()
                })
            }
        )
    };

    // (property name, field name on token variant inner type)
    (@($ctxs:ident; $field_name:ident; $token_variant:path $(| $other_variant:path)*;
        ($prop_name:ident, $field:ident $(,)? ) $($rest:tt)*
    ), $($arms:tt)*) => {
        $crate::property_group!(
            @($ctxs; $field_name; $token_variant $(| $other_variant:path)*; $($rest)*),
            $($arms)*
            stringify!($prop_name) => {
                $crate::property_stub!($ctxs, $token_variant $(| $other_variant)*, token, {
                    token.$field.clone().into()
                })
            }
        )
    };

    // (property name, destructured token variant, handler block)
    (@($ctxs:ident; $field_name:ident; $token_variant:path $(| $other_variant:path)*;
        ($prop_name:ident, $token:ident, $impl:block $(,)? ) $($rest:tt)*
    ), $($arms:tt)*) => {
        $crate::property_group!(
            @($ctxs; $field_name; $token_variant $(| $other_variant)*; $($rest)*),
            $($arms)*
            stringify!($prop_name) => {
                $crate::property_stub!($ctxs, $token_variant $(| $other_variant)*, $token, $impl)
            }
        )
    };

    // final case
    (@($ctxs:ident; $field_name:ident; $token_variant:path $(| $other_variant:path)*; ),
        $($arms:tt)+
    ) => {
        match $field_name.as_ref() {
            $($arms)+
            _ => unreachable!(
                "Unexpected property name {} for token variant {:?}",
                $field_name.as_ref(), stringify!($token_variant $(| $other_variant:path)*)
            ),
        }
    }
}

#[macro_export]
macro_rules! project_property {
    ($ctxs:ident, $type_name:ident, $field_name:ident,
        [
            $(
                {
                    $type_option:ident $(| $other_type_option:ident)*,
                    $token_variant:path $(| $other_variant:path)*,
                    [ $($rest:tt),+ $(,)? ] $(,)?
                }
            ),+ $(,)?
        ] $(,)?
    ) => {
        match $type_name.as_ref() {
            $(
                stringify!($type_option) $(| stringify!($other_type_option))* => {
                    $crate::property_group!($ctxs, $field_name, $token_variant $(| $other_variant)*, [ $($rest),+ ])
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
    ($ctxs:ident, $lt:lifetime, $token_variant:path $(| $other_variant:path)*, $token:ident, $impl:tt) => {
        Box::new($ctxs.map(move |ctx| {
            let neighbors: Box<dyn Iterator<Item = Self::DataToken> + $lt> =
                match &ctx.current_token {
                    Some($token_variant($token)) => $impl,
                    $( Some($other_variant($token)) => $impl, )*
                    None => Box::new(std::iter::empty()),

                    // If there's only one token variant, the below pattern is unreachable.
                    // We don't want to cause a lint for the user of the macro, so we suppress it.
                    #[allow(unreachable_patterns)]
                    Some(x) => {
                        unreachable!(
                            "Unexpected token variant encountered! Expecting {} but got {:?}",
                            stringify!($token_variant $(| $other_variant)*),
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
    ($ctxs:ident, $lt:lifetime, $edge_name_var:ident, $token_variant:path $(| $other_variant:path)*,
        [
            $($rest:tt),+ $(,)?
        ] $(,)?
    ) => {
        $crate::neighbor_group!( @( $ctxs; $lt; $edge_name_var; $token_variant $(| $other_variant)*; $($rest)+ ), )
    };

    // edge name in schema matches field name on token variant inner type,
    // so matching (edge_and_field, resulting token variant)
    (@($ctxs:ident; $lt:lifetime; $edge_name_var:ident; $token_variant:path $(| $other_variant:path)*;
        (
            $edge_and_field:ident,
            $next_variant:path $(,)?
        ) $($rest:tt)*
    ), $($arms:tt)*) => {
        $crate::neighbor_group!(
            @($ctxs; $lt; $edge_name_var; $token_variant $(| $other_variant)*; $($rest)*),
            $($arms)*
            stringify!($edge_and_field) => {
                $crate::neighbor_stub!($ctxs, $lt, $token_variant, token, {
                    Box::new(token.$edge_and_field.iter().map($next_variant))
                })
            }
        )
    };

    // (edge name, field name on token variant inner type, resulting token variant)
    (@($ctxs:ident; $lt:lifetime; $edge_name_var:ident; $token_variant:path $(| $other_variant:path)*;
        (
            $edge_name:ident,
            $field:ident,
            $next_variant:path $(,)?
        ) $($rest:tt)*
    ), $($arms:tt)*) => {
        $crate::neighbor_group!(
            @($ctxs; $lt; $edge_name_var; $token_variant $(| $other_variant)*; $($rest)*),
            $($arms)*
            stringify!($edge_name) => {
                $crate::neighbor_stub!($ctxs, $lt, $token_variant $(| $other_variant)*, token, {
                    Box::new(token.$field.iter().map($next_variant))
                })
            }
        )
    };

    // (edge name, destructured token variant, handler block)
    (@($ctxs:ident; $lt:lifetime; $edge_name_var:ident; $token_variant:path $(| $other_variant:path)*;
        (
            $edge_name:ident,
            $token:ident,
            $impl:block $(,)?
        ) $($rest:tt)*
    ), $($arms:tt)*) => {
        $crate::neighbor_group!(
            @($ctxs; $lt; $edge_name_var; $token_variant $(| $other_variant)*; $($rest)*),
            $($arms)*
            stringify!($edge_name) => {
                $crate::neighbor_stub!($ctxs, $lt, $token_variant $(| $other_variant)*, $token, $impl)
            }
        )
    };

    // final case
    (@($ctxs:ident; $lt:lifetime; $edge_name_var:ident; $token_variant:path $(| $other_variant:path)*; ),
        $($arms:tt)+
    ) => {
        match $edge_name_var.as_ref() {
            $($arms)+
            _ => unreachable!(
                "Unexpected edge name {} for token variant {:?}",
                $edge_name_var.as_ref(), stringify!($token_variant $(| $other_variant)*)
            ),
        }
    }
}

#[macro_export]
macro_rules! project_neighbors {
    ($ctxs:ident, $lt:lifetime, $type_name_var:ident, $edge_name_var:ident,
        [
            $(
                {
                    $type_option:ident $(| $other_type_option:ident)*,
                    $token_variant:path $(| $other_variant:path)*,
                    [ $($rest:tt),+ $(,)? ] $(,)?
                }
            ),+ $(,)?
        ] $(,)?
    ) => {
        match $type_name_var.as_ref() {
            $(
                stringify!($type_option) $(| stringify!($other_type_option))* => {
                    $crate::neighbor_group!($ctxs, $lt, $edge_name_var, $token_variant $(| $other_variant)*, [ $($rest),+ ])
                }
            )+
            _ => unreachable!(
                "Unexpected type name {}", $type_name_var.as_ref()
            ),
        }
    };
}

#[macro_export]
macro_rules! project_neighbors2_match_arm {
    // token field is same as edge name, so this is just reading the next variant
    ($ctxs:ident, $lt:lifetime, $edge_name:ident, $token_variant:path $(| $other_variant:path)*, $next_variant:path $(,)?) => {
        $crate::neighbor_stub!(
            $ctxs, $lt, $token_variant $(| $other_variant)*, token, {
                Box::new(token.$edge_name.iter().map($next_variant))
            }
        )
    };

    // (token field, next variant)
    ($ctxs:ident, $lt:lifetime, $edge_name:ident, $token_variant:path $(| $other_variant:path)*, ($field:ident, $next_variant:path $(,)?) $(,)?) => {
        $crate::neighbor_stub!(
            $ctxs, $lt, $token_variant $(| $other_variant)*, token, {
                Box::new(token.$field.iter().map($next_variant))
            }
        )
    };

    // (destuctured token var, impl block)
    ($ctxs:ident, $lt:lifetime, $edge_name:ident, $token_variant:path $(| $other_variant:path)*, ($token:ident, $impl:block $(,)?) $(,)?) => {
        $crate::neighbor_stub!($ctxs, $lt, $token_variant $(| $other_variant)*, $token, $impl)
    };
}

// TODO: figure out whether we want both this macro and the other neighbors macro, or just one
#[macro_export]
macro_rules! project_neighbors2 {
    ($ctxs:ident, $lt:lifetime, $type_name_var:ident, $edge_name_var:ident,
        [
            $(
                {
                    $type_option:ident $(| $other_type_option:ident)*,
                    $edge_name:ident,
                    $token_variant:path $(| $other_variant:path)*,
                    $rest:tt $(,)?
                }
            ),+ $(,)?
        ] $(,)?
    ) => {
        match ($edge_name_var.as_ref(), $type_name_var.as_ref()) {
            $(
                (stringify!($edge_name), stringify!($type_option)) $(| (stringify!($edge_name), stringify!($other_type_option)))* => {
                    $crate::project_neighbors2_match_arm!($ctxs, $lt, $edge_name, $token_variant $(| $other_variant)*, $rest )
                }
            )+
            _ => unreachable!(
                "Unexpected type and edge name {} {}", $type_name_var.as_ref(), $edge_name_var.as_ref(),
            ),
        }
    };
}
