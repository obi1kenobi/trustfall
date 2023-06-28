#[derive(Debug, Clone, trustfall::provider::TrustfallEnumVertex)]
pub enum Vertex {
    Webpage(()),
    Story(()),
    Item(()),
    Job(()),
    Comment(()),
    User(()),
}
