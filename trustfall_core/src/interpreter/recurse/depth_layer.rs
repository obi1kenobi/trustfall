use std::{cell::{RefMut, RefCell}, fmt::Debug, rc::Rc, iter::Fuse, marker::PhantomData, collections::VecDeque};

use crate::interpreter::DataContext;

use super::NeighborsBundle;

pub(super) trait RecursionLayer<'token, Token: Clone + Debug + 'token> : Iterator<Item = DataContext<Token>> + 'token {
    fn total_pulls(&self) -> usize;

    fn total_prepared(&self) -> usize;

    fn prepare_without_pull(&mut self) -> Option<DataContext<Token>>;

    fn prepare_with_pull(&mut self) -> Option<DataContext<Token>>;

    fn pop_passed_unprepared(&mut self) -> Option<DataContext<Token>>;
}

/// Cloneable wrapper with interior mutability. You can have two references to
/// it and call next() on either, just not at the same time.
pub(super) struct RcRecursionLayer<'token, Token: Clone + Debug + 'token, R: RecursionLayer<'token, Token>> {
    inner: Rc<RefCell<R>>,
    _marker: PhantomData<&'token Token>,
}

impl<'token, Token: Clone + Debug + 'token, R: RecursionLayer<'token, Token>> Iterator for RcRecursionLayer<'token, Token, R> {
    type Item = <R as Iterator>::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.as_ref().borrow_mut().next()
    }
}

impl<'token, Token: Clone + Debug + 'token, R: RecursionLayer<'token, Token>> RecursionLayer<'token, Token> for RcRecursionLayer<'token, Token, R> {
    fn total_pulls(&self) -> usize {
        self.inner.as_ref().borrow().total_pulls()
    }

    fn total_prepared(&self) -> usize {
        self.inner.as_ref().borrow().total_prepared()
    }

    fn prepare_without_pull(&mut self) -> Option<DataContext<Token>> {
        let mut ref_mut: RefMut<_> = self.inner.as_ref().borrow_mut();
        ref_mut.prepare_without_pull()
    }

    fn prepare_with_pull(&mut self) -> Option<DataContext<Token>> {
        let mut ref_mut: RefMut<_> = self.inner.as_ref().borrow_mut();
        ref_mut.prepare_with_pull()
    }

    fn pop_passed_unprepared(&mut self) -> Option<DataContext<Token>> {
        let mut ref_mut: RefMut<_> = self.inner.as_ref().borrow_mut();
        ref_mut.pop_passed_unprepared()
    }
}

/// One layer of the @recurse evaluation. Each @recurse starts with a Layer::DepthZero layer,
/// followed by Layer::Neighbors layers equal to the depth.
///
/// For example: `@recurse(depth: 1)` produces one Layer::DepthZero and one Layer::Neighbors layer.
pub(super) enum Layer<'token, Token>
where
    Token: Clone + Debug + 'token,
{
    /// the "depth 0" of the recursion, where the initial nodes are returned
    DepthZero(RcRecursionLayer<'token, Token, DepthZeroReader<'token, Token>>),

    /// a "recursive step" where neighboring vertices are resolved and returned
    Neighbors(RcRecursionLayer<'token, Token, BundleReader<'token, Token>>),
}

impl<'token, Token: Clone + Debug + 'token> Iterator for Layer<'token, Token> {
    type Item = DataContext<Token>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Layer::DepthZero(d) => d.next(),
            Layer::Neighbors(n) => n.next(),
        }
    }
}

impl<'token, Token: Clone + Debug + 'token> RecursionLayer<'token, Token> for Layer<'token, Token> {
    #[inline]
    fn total_pulls(&self) -> usize {
        match self {
            Layer::DepthZero(d) => d.total_pulls(),
            Layer::Neighbors(n) => n.total_pulls(),
        }
    }

    #[inline]
    fn total_prepared(&self) -> usize {
        match self {
            Layer::DepthZero(d) => d.total_prepared(),
            Layer::Neighbors(n) => n.total_prepared(),
        }
    }

    #[inline]
    fn prepare_without_pull(&mut self) -> Option<DataContext<Token>> {
        match self {
            Layer::DepthZero(d) => d.prepare_without_pull(),
            Layer::Neighbors(n) => n.prepare_without_pull(),
        }
    }

    #[inline]
    fn prepare_with_pull(&mut self) -> Option<DataContext<Token>> {
        match self {
            Layer::DepthZero(d) => d.prepare_with_pull(),
            Layer::Neighbors(n) => n.prepare_with_pull(),
        }
    }

    #[inline]
    fn pop_passed_unprepared(&mut self) -> Option<DataContext<Token>> {
        match self {
            Layer::DepthZero(d) => d.pop_passed_unprepared(),
            Layer::Neighbors(n) => n.pop_passed_unprepared(),
        }
    }
}

pub(super) struct BundleReader<'token, Token>
where
    Token: Clone + Debug + 'token,
{
    coercion_iter: Fuse<Box<dyn Iterator<Item = (DataContext<Token>, bool)> + 'token>>,
    inner: Fuse<NeighborsBundle<'token, Token>>,

    /// The source context and neighbors that we're in the middle of expanding.
    source: Option<DataContext<Token>>,
    buffer: Fuse<Box<dyn Iterator<Item = Token> + 'token>>,

    /// Number of times pulled buffers from the bundle
    total_pulls: usize,

    /// Number of outputs so far
    total_prepared: usize,

    /// Items already seen (and probably recorded as outputs), but not yet pulled from next().
    prepared: VecDeque<DataContext<Token>>,

    /// Items pulled from next() without being prepared first. This can happen with batching.
    passed_unprepared: VecDeque<DataContext<Token>>,
}

impl<'token, Token: Debug + Clone + 'token> RecursionLayer<'token, Token> for BundleReader<'token, Token> {
    fn pop_passed_unprepared(&mut self) -> Option<DataContext<Token>> {
        let maybe_token = self.passed_unprepared.pop_front();
        if maybe_token.is_some() {
            self.total_prepared += 1;
        }
        maybe_token
    }

    fn total_prepared(&self) -> usize {
        self.total_prepared
    }

    fn total_pulls(&self) -> usize {
        self.total_pulls
    }

    fn prepare_without_pull(&mut self) -> Option<DataContext<Token>> {
        self.prepare(0)
    }

    fn prepare_with_pull(&mut self) -> Option<DataContext<Token>> {
        self.prepare(1)
    }
}

impl<'token, Token> Debug for BundleReader<'token, Token>
where
    Token: Debug + Clone + 'token,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BundleReader")
            .field("coercion_iter", &"<elided>")
            .field("inner", &"<elided>")
            .field("source", &self.source)
            .field("buffer", &"<elided>")
            .field("total_pulls", &self.total_pulls)
            .field("total_prepared", &self.total_prepared)
            .field("prepared", &self.prepared)
            .field("passed_unprepared", &self.passed_unprepared)
            .finish()
    }
}

impl<'token, Token> BundleReader<'token, Token>
where
    Token: Clone + Debug + 'token,
{
    pub(super) fn new(
        coercion_iter: Box<dyn Iterator<Item = (DataContext<Token>, bool)> + 'token>,
        bundle: NeighborsBundle<'token, Token>,
    ) -> Self {
        let buffer: Box<dyn Iterator<Item = _> + 'token> = Box::new(std::iter::empty());
        Self {
            coercion_iter: coercion_iter.fuse(),
            inner: bundle.fuse(),
            source: None,
            buffer: buffer.fuse(),
            total_pulls: 0,
            total_prepared: 0,
            prepared: Default::default(),
            passed_unprepared: Default::default(),
        }
    }

    /// Prepare while not pulling more than specified from the bundle,
    /// i.e. from the parent level of the recursion.
    pub(super) fn prepare(&mut self, mut pull_limit: usize) -> Option<DataContext<Token>> {
        loop {
            let maybe_token = self.pop_passed_unprepared();
            if maybe_token.is_some() {
                return maybe_token;
            }

            if let Some(token) = self.buffer.next() {
                let neighbor_ctx = self
                    .source
                    .as_ref()
                    .expect("no source for existing buffer")
                    .split_and_move_to_token(Some(token));
                self.prepared.push_back(neighbor_ctx.clone());
                self.total_prepared += 1;
                return Some(neighbor_ctx);
            }

            if pull_limit == 0 {
                return None;
            }
            if let Some((_ctx, can_coerce)) = self.coercion_iter.next() {
                self.total_pulls += 1;
                if can_coerce {
                    // This element passes through the coercion,
                    // and will also be returned by self.inner.next().
                    let (new_source, new_buffer) = self.inner.next().expect(
                        "coercion returned coercible element but inner.next() returned None",
                    );
                    // debug_assert_eq!(ctx, new_source);
                    self.source = Some(new_source);
                    self.buffer = new_buffer.fuse();
                }
            } else {
                return None;
            }
            pull_limit -= 1;
        }
    }
}

impl<'token, Token> Iterator for BundleReader<'token, Token>
where
    Token: Clone + Debug + 'token,
{
    type Item = DataContext<Token>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let maybe_token = self.prepared.pop_front();
            if maybe_token.is_some() {
                // We found a prepared element, return it.
                return maybe_token;
            }

            // If the adapter isn't using batching, then the next() call here will always be None
            // because any elements from the buffer were processed in a prior prepare().
            //
            // If the adapter is using batching, we might end up pulling from the buffer here,
            // so we'll also have to put those items back in the queue for prepare().
            // If we forgot to do that, those intermediate nodes in the recursion would not
            // be included in the recursive expansion of the edge.
            let maybe_token = self.buffer.next();
            if let Some(token) = maybe_token {
                let neighbor_ctx = self
                    .source
                    .as_ref()
                    .expect("no source for existing buffer")
                    .split_and_move_to_token(Some(token));
                self.passed_unprepared.push_back(neighbor_ctx.clone());
                return Some(neighbor_ctx);
            }

            loop {
                if let Some((_ctx, can_coerce)) = self.coercion_iter.next() {
                    self.total_pulls += 1;
                    if can_coerce {
                        // This element passes through the coercion,
                        // and will also be returned by self.inner.next().
                        let (new_source, new_buffer) = self.inner.next().expect(
                            "coercion returned coercible element but inner.next() returned None",
                        );
                        // debug_assert_eq!(ctx, new_source);
                        self.source = Some(new_source);
                        self.buffer = new_buffer.fuse();
                        break;
                    }
                } else {
                    // No more data.
                    return None;
                }
            }
        }
    }
}

pub(super) struct DepthZeroReader<'token, Token>
where
    Token: Clone + Debug + 'token,
{
    inner: Fuse<Box<dyn Iterator<Item=DataContext<Token>> + 'token>>,

    /// Number of times pulled buffers from the bundle
    total_pulls: usize,

    /// Number of outputs so far
    total_prepared: usize,

    /// Items already seen (and probably recorded as outputs), but not yet pulled from next().
    prepared: VecDeque<DataContext<Token>>,

    /// Items pulled from next() without being prepared first. This can happen with batching.
    passed_unprepared: VecDeque<DataContext<Token>>,
}

impl<'token, Token: Debug + Clone + 'token> DepthZeroReader<'token, Token> {
    fn new(inner: Box<dyn Iterator<Item = DataContext<Token>> + 'token>) -> Self {
        Self {
            inner: inner.fuse(),
            total_pulls: 0,
            total_prepared: 0,
            prepared: Default::default(),
            passed_unprepared: Default::default(),
        }
    }
}

impl<'token, Token: Debug + Clone + 'token> RecursionLayer<'token, Token> for DepthZeroReader<'token, Token> {
    fn pop_passed_unprepared(&mut self) -> Option<DataContext<Token>> {
        let maybe_token = self.passed_unprepared.pop_front();
        if maybe_token.is_some() {
            self.total_prepared += 1;
        }
        maybe_token
    }

    /// Prepare an element. There's no "pull limit" unlike in BundleReader;
    /// this method merely helps us track whether the next() calls were out of order or not.
    fn prepare_without_pull(&mut self) -> Option<DataContext<Token>> {
        let maybe_token = self.pop_passed_unprepared();
        if maybe_token.is_some() {
            return maybe_token;
        }

        let maybe_ctx = self.inner.next();
        if maybe_ctx.is_some() {
            self.total_prepared += 1;
            self.total_pulls += 1;
        }

        maybe_ctx
    }

    #[inline]
    fn prepare_with_pull(&mut self) -> Option<DataContext<Token>> {
        None
    }

    fn total_prepared(&self) -> usize {
        self.total_prepared
    }

    fn total_pulls(&self) -> usize {
        self.total_pulls
    }
}

impl<'token, Token: Debug + Clone + 'token> Iterator for DepthZeroReader<'token, Token> {
    type Item = DataContext<Token>;

    fn next(&mut self) -> Option<Self::Item> {
        let maybe_ctx = self.prepared.pop_front();
        if maybe_ctx.is_some() {
            // We found a prepared element, return it.
            return maybe_ctx;
        }

        let maybe_ctx = self.inner.next();
        if let Some(ref ctx) = maybe_ctx {
            self.total_pulls += 1;
            self.passed_unprepared.push_back(ctx.clone());
        }

        maybe_ctx
    }
}
