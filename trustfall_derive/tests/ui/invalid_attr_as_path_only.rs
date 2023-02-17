use trustfall_derive::TrustfallEnumVertex;

#[derive(Debug, Clone, TrustfallEnumVertex)]
enum TwoVariants {
    #[trustfall]
    First,
    Second,
}

fn main() {}
