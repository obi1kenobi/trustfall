use trustfall_derive::TrustfallEnumVertex;

#[derive(Debug, Clone, TrustfallEnumVertex)]
enum Vertex {
    First,
    #[trustfall(skip_conversion = "yes")]
    Second,
}

fn main() {}
