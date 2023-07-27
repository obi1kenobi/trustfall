#[non_exhaustive]
#[derive(Debug, Clone, trustfall::provider::TrustfallEnumVertex)]
pub enum Vertex {
    Const(()),
    Const2(()),
    Continue(()),
    Continue2(()),
    Dyn(()),
    Dyn2(()),
    If(()),
    If2(()),
    Mod(()),
    Mod2(()),
    Self_(()),
    Self2(()),
    Type(()),
    Type2(()),
    Unsafe(()),
    Unsafe2(()),
    Where(()),
    Where2(()),
}
