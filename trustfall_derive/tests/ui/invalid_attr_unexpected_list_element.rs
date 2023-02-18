use trustfall_derive::TrustfallEnumVertex;

#[derive(Debug, Clone, TrustfallEnumVertex)]
enum One {
    #[trustfall(skip_conversion, unexpected_arg)]
    First,
    Second,
}

#[derive(Debug, Clone, TrustfallEnumVertex)]
enum Other {
    First,
    #[trustfall(unexpected_arg, skip_conversion)]
    Second,
}

fn main() {}
