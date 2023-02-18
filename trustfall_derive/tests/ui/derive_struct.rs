use trustfall_derive::{TrustfallEnumVertex, Typename};

#[derive(Debug, Clone, TrustfallEnumVertex)]
struct Vertex {
    content: i64,
}

#[derive(Debug, Clone, Typename)]
struct OtherVertex {
    content: i64,
}

fn main() {}
