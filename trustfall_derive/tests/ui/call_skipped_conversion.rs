use trustfall_core::interpreter::Typename;
use trustfall_derive::TrustfallEnumVertex;

#[derive(Debug, Clone, TrustfallEnumVertex)]
enum TwoVariants {
    #[trustfall(skip_conversion)]
    First,
    Second,
}

fn main() {
    let first = TwoVariants::First;
    assert_eq!("First", first.typename());
    assert_eq!(None, first.as_second());

    // this method should not exist, expecting an error here
    first.as_first();

    let second = TwoVariants::Second;
    assert_eq!("Second", second.typename());
    assert_eq!(Some(()), second.as_second());

    // this method should not exist, expecting an error here
    second.as_first();
}
