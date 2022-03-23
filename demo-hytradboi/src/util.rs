pub(crate) enum PagerOutput<T> {
    None,
    KnownFinalPage(std::vec::IntoIter<T>),
    Page(std::vec::IntoIter<T>),
}

pub(crate) trait Pager {
    type Item;

    fn get_page(&mut self, page: usize) -> PagerOutput<Self::Item>;

    fn into_iter(self) -> PaginationIterator<Self::Item, Self> where Self : Sized {
        PaginationIterator::new(self)
    }
}

pub(crate) struct PaginationIterator<T, P: Pager<Item=T>> {
    pager: P,
    next_page: usize,
    batch: Option<std::vec::IntoIter<T>>,
    final_page_seen: bool,
}

impl<T, P: Pager<Item=T>> PaginationIterator<T, P> {
    pub(crate) fn new(pager: P) -> Self {
        Self {
            pager,
            next_page: 1,
            batch: None,
            final_page_seen: false,
        }
    }
}

impl<T, P: Pager<Item=T>> Iterator for PaginationIterator<T, P> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let needs_next_page = match self.batch.take() {
                Some(mut iter) => {
                    let next = iter.next();
                    if next.is_some() {
                        self.batch = Some(iter);
                        break next;
                    } else {
                        true
                    }
                }
                None => {
                    true
                }
            };
            if needs_next_page {
                if self.final_page_seen {
                    break None;
                } else {
                    match self.pager.get_page(self.next_page) {
                        PagerOutput::None => {
                            self.final_page_seen = true;
                            self.batch = None;
                        }
                        PagerOutput::KnownFinalPage(batch) => {
                            self.final_page_seen = true;
                            self.batch = Some(batch);
                        },
                        PagerOutput::Page(batch) => {
                            self.batch = Some(batch);
                        },
                    }
                }
            }
        }
    }
}
