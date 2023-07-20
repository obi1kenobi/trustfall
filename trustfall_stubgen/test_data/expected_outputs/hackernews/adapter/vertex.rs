#[non_exhaustive]
#[derive(Debug, Clone, trustfall::provider::TrustfallEnumVertex)]
pub enum Vertex {
    Comment(()),
    Item(()),
    Job(()),
    Story(()),
    User(()),
    Webpage(()),
}
