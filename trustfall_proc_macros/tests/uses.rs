use trustfall_proc_macros::VariantsAsVertexTypes;

// TODO: test for UI errors due to:
// - not an enum
// - enum is not Debug
// - enum is not Clone

#[test]
fn empty_enum() {
    #[derive(Debug, Clone, VariantsAsVertexTypes)]
    enum NoVariants {}
}

#[test]
fn single_unit_variant() {
    #[derive(Debug, Clone, VariantsAsVertexTypes)]
    enum SingleUnitVariant {
        Foo,
    }

    let value = SingleUnitVariant::Foo;
    assert_eq!("Foo", value.typename());
    assert_eq!(Some(()), value.as_foo());
}

#[test]
fn two_unit_variants() {
    #[derive(Debug, Clone, VariantsAsVertexTypes)]
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
    #[derive(Debug, Clone, VariantsAsVertexTypes)]
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
    #[derive(Debug, Clone, VariantsAsVertexTypes)]
    enum TwoVariants {
        First,
        Second(&'static str, Vec<usize>),
    }

    let first = TwoVariants::First;
    assert_eq!("First", first.typename());
    assert_eq!(Some(()), first.as_first());
    assert_eq!(None, first.as_second());

    let second = TwoVariants::Second("fixed", vec![1, 2]);
    assert_eq!("Second", second.typename());
    assert_eq!(None, second.as_first());
    assert_eq!(Some((&"fixed", &vec![1, 2])), second.as_second());
}
