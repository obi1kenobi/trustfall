#![no_main]
use libfuzzer_sys::fuzz_target;

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::io::Cursor;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use byteorder::{LittleEndian, ReadBytesExt};
use globset::GlobBuilder;
use serde::Deserialize;
use walkdir::WalkDir;

extern crate trustfall_core;

use trustfall_core::{
    interpreter::{execution::interpret_ir, Adapter, AsVertex},
    ir::{FieldValue, IndexedQuery},
};

mod numbers_adapter;

struct VariableChunkIterator<I: Iterator> {
    iter: I,
    buffer: VecDeque<I::Item>,
    chunk_sequence: u64,
    offset: usize,
}

impl<I: Iterator> VariableChunkIterator<I> {
    fn new(iter: I, chunk_sequence: u64) -> Self {
        let mut value =
            Self { iter, buffer: VecDeque::with_capacity(4), chunk_sequence, offset: 0 };

        // Eagerly advancing the input iterator is important
        // because that's how we reproduce: https://github.com/obi1kenobi/trustfall/issues/205
        let chunk_size = value.next_chunk_size();
        value.buffer.extend(value.iter.by_ref().take(chunk_size));
        value
    }

    fn next_chunk_size(&mut self) -> usize {
        let next_chunk = ((self.chunk_sequence >> self.offset) & 3) + 1;
        if self.offset >= 62 {
            self.offset = 0;
        } else {
            self.offset += 2;
        }
        assert!(next_chunk >= 1);
        next_chunk as usize
    }
}

impl<I: Iterator> Iterator for VariableChunkIterator<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(element) = self.buffer.pop_front() {
            Some(element)
        } else {
            let next = self.iter.next();
            if next.is_some() {
                let elements_to_buffer = self.next_chunk_size() - 1;
                self.buffer.extend(self.iter.by_ref().take(elements_to_buffer));
            }
            next
        }
    }
}

struct VariableBatchingAdapter<'a, AdapterT: Adapter<'a> + 'a> {
    adapter: AdapterT,
    cursor: RefCell<Cursor<&'a [u8]>>,
    _marker: PhantomData<&'a ()>,
}

impl<'a, AdapterT: Adapter<'a> + 'a> VariableBatchingAdapter<'a, AdapterT> {
    fn new(adapter: AdapterT, cursor: Cursor<&'a [u8]>) -> Self {
        Self { adapter, cursor: RefCell::new(cursor), _marker: PhantomData }
    }
}

impl<'a, AdapterT: Adapter<'a> + 'a> Adapter<'a> for VariableBatchingAdapter<'a, AdapterT> {
    type Vertex = AdapterT::Vertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &trustfall_core::ir::EdgeParameters,
        resolve_info: &trustfall_core::interpreter::ResolveInfo,
    ) -> trustfall_core::interpreter::VertexIterator<'a, Self::Vertex> {
        let mut cursor_ref = self.cursor.borrow_mut();
        let sequence = cursor_ref.read_u64::<LittleEndian>().unwrap_or(0);
        drop(cursor_ref);

        let inner = self.adapter.resolve_starting_vertices(edge_name, parameters, resolve_info);
        Box::new(VariableChunkIterator::new(inner, sequence))
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: trustfall_core::interpreter::ContextIterator<'a, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &trustfall_core::interpreter::ResolveInfo,
    ) -> trustfall_core::interpreter::ContextOutcomeIterator<'a, V, FieldValue> {
        let mut cursor_ref = self.cursor.borrow_mut();
        let sequence = cursor_ref.read_u64::<LittleEndian>().unwrap_or(0);
        drop(cursor_ref);

        let inner = self.adapter.resolve_property(
            Box::new(contexts),
            type_name,
            property_name,
            resolve_info,
        );
        Box::new(VariableChunkIterator::new(inner, sequence))
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: trustfall_core::interpreter::ContextIterator<'a, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &trustfall_core::ir::EdgeParameters,
        resolve_info: &trustfall_core::interpreter::ResolveEdgeInfo,
    ) -> trustfall_core::interpreter::ContextOutcomeIterator<
        'a,
        V,
        trustfall_core::interpreter::VertexIterator<'a, Self::Vertex>,
    > {
        let mut cursor_ref = self.cursor.borrow_mut();
        let sequence = cursor_ref.read_u64::<LittleEndian>().unwrap_or(0);
        drop(cursor_ref);

        let inner = self.adapter.resolve_neighbors(
            contexts,
            type_name,
            edge_name,
            parameters,
            resolve_info,
        );
        Box::new(VariableChunkIterator::new(inner, sequence))
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: trustfall_core::interpreter::ContextIterator<'a, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &trustfall_core::interpreter::ResolveInfo,
    ) -> trustfall_core::interpreter::ContextOutcomeIterator<'a, V, bool> {
        let mut cursor_ref = self.cursor.borrow_mut();
        let sequence = cursor_ref.read_u64::<LittleEndian>().unwrap_or(0);
        drop(cursor_ref);

        let inner =
            self.adapter.resolve_coercion(contexts, type_name, coerce_to_type, resolve_info);
        Box::new(VariableChunkIterator::new(inner, sequence))
    }
}

#[derive(Debug, Clone)]
struct TestCase<'a> {
    query: Arc<IndexedQuery>,
    arguments: Arc<BTreeMap<Arc<str>, FieldValue>>,
    cursor: Cursor<&'a [u8]>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TestIRQuery {
    pub(crate) schema_name: String,

    pub(crate) ir_query: trustfall_core::ir::IRQuery,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) arguments: BTreeMap<String, FieldValue>,
}

type QueryAndArgs = (Arc<IndexedQuery>, Arc<BTreeMap<Arc<str>, FieldValue>>);

static QUERY_DATA: OnceLock<Vec<QueryAndArgs>> = OnceLock::new();

fn get_query_data() -> &'static Vec<QueryAndArgs> {
    QUERY_DATA.get_or_init(|| {
        let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        buf.pop();
        buf.push("test_data/tests/valid_queries");
        let base = buf.as_path();

        let glob = GlobBuilder::new("*.ir.ron")
            .case_insensitive(true)
            .literal_separator(true)
            .build()
            .unwrap()
            .compile_matcher();

        let walker = WalkDir::new(base);
        let mut paths = vec![];

        for file in walker {
            let file = file.unwrap();
            let path = file.path();
            let stripped_path = path.strip_prefix(base).unwrap();
            if !glob.is_match(stripped_path) {
                continue;
            }
            paths.push(PathBuf::from(path));
        }
        paths.sort_unstable();

        let mut outputs = vec![];
        for path in paths {
            let contents = std::fs::read_to_string(path).expect("failed to read file");
            let input_data: TestIRQuery = ron::from_str::<Result<TestIRQuery, ()>>(&contents)
                .expect("failed to parse file")
                .expect("Err result");
            if input_data.schema_name == "numbers" {
                let indexed_query = Arc::new(input_data.ir_query.try_into().unwrap());
                let arguments = Arc::new(
                    input_data.arguments.into_iter().map(|(k, v)| (k.into(), v)).collect(),
                );
                outputs.push((indexed_query, arguments));
            }
        }

        outputs
    })
}

impl<'a> TryFrom<&'a [u8]> for TestCase<'a> {
    type Error = ();

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        if value.len() < 2 {
            return Err(());
        };
        let (file_selector_bytes, rest) = value.split_at(2);
        let file_selector = u16::from_le_bytes(file_selector_bytes.try_into().unwrap()) as usize;
        let (query, args) = get_query_data().get(file_selector).ok_or(())?;
        let cursor = Cursor::new(rest);

        Ok(Self { query: query.clone(), arguments: args.clone(), cursor })
    }
}

fn execute_query_with_fuzzed_batching(test_case: TestCase<'_>) {
    #[allow(clippy::arc_with_non_send_sync)]
    let adapter =
        Arc::new(VariableBatchingAdapter::new(numbers_adapter::NumbersAdapter, test_case.cursor));
    interpret_ir(adapter, test_case.query, test_case.arguments).unwrap().for_each(drop);
}

fuzz_target!(|data: &[u8]| {
    if let Ok(test_case) = data.try_into() {
        execute_query_with_fuzzed_batching(test_case);
    }
});
