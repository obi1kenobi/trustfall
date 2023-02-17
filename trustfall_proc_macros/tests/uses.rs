use trustfall_proc_macros::TrustfallEnumVertex;

#[test]
fn empty_enum() {
    #[derive(Debug, Clone, TrustfallEnumVertex)]
    enum NoVariants {}
}

#[test]
fn single_unit_variant() {
    #[derive(Debug, Clone, TrustfallEnumVertex)]
    enum SingleUnitVariant {
        Foo,
    }

    let value = SingleUnitVariant::Foo;
    assert_eq!("Foo", value.typename());
    assert_eq!(Some(()), value.as_foo());
}

#[test]
fn snake_case() {
    #[derive(Debug, Clone, TrustfallEnumVertex)]
    enum SingleUnitVariant {
        ShouldBecomeSnakeCase,
        Includes1Numbers2,
        #[allow(non_camel_case_types)] AlreadyPartly_SnakeCase,
        ConsecutiveCapsEGLikeID,
    }

    let value = SingleUnitVariant::ShouldBecomeSnakeCase;
    assert_eq!("ShouldBecomeSnakeCase", value.typename());
    assert_eq!(Some(()), value.as_should_become_snake_case());

    let value = SingleUnitVariant::Includes1Numbers2;
    assert_eq!("Includes1Numbers2", value.typename());
    assert_eq!(Some(()), value.as_includes1_numbers2());

    let value = SingleUnitVariant::AlreadyPartly_SnakeCase;
    assert_eq!("AlreadyPartly_SnakeCase", value.typename());
    assert_eq!(Some(()), value.as_already_partly_snake_case());

    let value = SingleUnitVariant::ConsecutiveCapsEGLikeID;
    assert_eq!("ConsecutiveCapsEGLikeID", value.typename());
    assert_eq!(Some(()), value.as_consecutive_caps_e_g_like_i_d());
}

#[test]
fn two_unit_variants() {
    #[derive(Debug, Clone, TrustfallEnumVertex)]
    enum TwoVariants {
        First,
        Second,
    }

    let first = TwoVariants::First;
    assert_eq!("First", first.typename());
    assert_eq!(Some(()), first.as_first());
    assert_eq!(None, first.as_second());

    let second = TwoVariants::Second;
    assert_eq!("Second", second.typename());
    assert_eq!(None, second.as_first());
    assert_eq!(Some(()), second.as_second());
}

#[test]
fn tuple_variants() {
    #[derive(Debug, Clone, TrustfallEnumVertex)]
    enum TwoVariants {
        First(i64),
        Second(&'static str, Vec<usize>),
    }

    let first = TwoVariants::First(123);
    assert_eq!("First", first.typename());
    assert_eq!(Some(&123), first.as_first());
    assert_eq!(None, first.as_second());

    let second = TwoVariants::Second("fixed", vec![1, 2]);
    assert_eq!("Second", second.typename());
    assert_eq!(None, second.as_first());
    assert_eq!(Some((&"fixed", &vec![1, 2])), second.as_second());
}

#[test]
fn mixed_variants() {
    #[derive(Debug, Clone, TrustfallEnumVertex)]
    enum TwoVariants {
        First,
        Second([&'static str; 2], Vec<usize>),
    }

    let first = TwoVariants::First;
    assert_eq!("First", first.typename());
    assert_eq!(Some(()), first.as_first());
    assert_eq!(None, first.as_second());

    let second = TwoVariants::Second(["fixed", "strings"], vec![1, 2]);
    assert_eq!("Second", second.typename());
    assert_eq!(None, second.as_first());
    assert_eq!(
        Some((&["fixed", "strings"], &vec![1, 2])),
        second.as_second()
    );
}

#[test]
fn struct_variant() {
    #[derive(Debug, Clone, TrustfallEnumVertex)]
    enum TwoVariants {
        FirstVariant {
            snake_case: String,
        },
        SecondVariant {
            a: i64,
            b: [&'static str; 2],
            c: Vec<usize>,
        },
    }

    let first = TwoVariants::FirstVariant {
        snake_case: "value".into(),
    };
    assert_eq!("FirstVariant", first.typename());
    assert_eq!(Some(&("value".into())), first.as_first_variant());
    assert_eq!(None, first.as_second_variant());

    let second = TwoVariants::SecondVariant {
        a: 42,
        b: ["fixed", "strings"],
        c: vec![1, 2],
    };
    assert_eq!("SecondVariant", second.typename());
    assert_eq!(None, second.as_first_variant());
    assert_eq!(
        Some((&42, &["fixed", "strings"], &vec![1, 2])),
        second.as_second_variant()
    );
}

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
