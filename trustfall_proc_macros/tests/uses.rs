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
    }

    let value = SingleUnitVariant::ShouldBecomeSnakeCase;
    assert_eq!("ShouldBecomeSnakeCase", value.typename());
    assert_eq!(Some(()), value.as_should_become_snake_case());
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
    assert_eq!(Some((&["fixed", "strings"], &vec![1, 2])), second.as_second());
}

#[test]
fn struct_variant() {
    #[derive(Debug, Clone, TrustfallEnumVertex)]
    enum TwoVariants {
        First {
            snake_case: String,
        },
        Second {
            a: i64,
            b: [&'static str; 2],
            c: Vec<usize>,
        },
    }

    let first = TwoVariants::First { snake_case: "value".into() };
    assert_eq!("First", first.typename());
    assert_eq!(Some(&("value".into())), first.as_first());
    assert_eq!(None, first.as_second());

    let second = TwoVariants::Second { a: 42, b: ["fixed", "strings"], c: vec![1, 2] };
    assert_eq!("Second", second.typename());
    assert_eq!(None, second.as_first());
    assert_eq!(Some((&42, &["fixed", "strings"], &vec![1, 2])), second.as_second());
}

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
