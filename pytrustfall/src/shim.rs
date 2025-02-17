use std::{collections::BTreeMap, sync::Arc};

use pyo3::{
    exceptions::PyStopIteration,
    prelude::*,
    types::{PyIterator, PyTuple},
    wrap_pyfunction,
};
use trustfall_core::{
    frontend::{error::FrontendError, parse},
    interpreter::{
        execution::interpret_ir, Adapter, AsVertex, ContextIterator as BaseContextIterator,
        ContextOutcomeIterator, DataContext, ResolveEdgeInfo, ResolveInfo, VertexIterator,
    },
    ir::{EdgeParameters, FieldValue as TrustfallFieldValue},
};

use crate::value::FieldValue;

pub(crate) fn register(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Schema>()?;
    m.add_class::<AdapterShim>()?;
    m.add_class::<ResultIterator>()?;
    m.add_class::<ContextIterator>()?;
    m.add_function(wrap_pyfunction!(interpret_query, m)?)?;
    Ok(())
}

#[pyclass(frozen)]
pub struct Schema {
    inner: trustfall_core::schema::Schema,
}

#[pymethods]
impl Schema {
    #[new]
    pub fn new(schema_text: &str) -> PyResult<Self> {
        let inner = trustfall_core::schema::Schema::parse(schema_text)
            .map_err(|e| crate::errors::InvalidSchemaError::new_err(format!("{e}")))?;

        Ok(Self { inner })
    }
}

// TODO: It would be nice if we gave a more complete error here including all bad args,
//       not just a piecemeal one that breaks on the first arg.
fn to_query_arguments(
    value: &Bound<'_, PyAny>,
) -> PyResult<Arc<BTreeMap<Arc<str>, TrustfallFieldValue>>> {
    let args = value.extract::<BTreeMap<String, FieldValue>>()?;
    Ok(Arc::new(args.into_iter().map(|(k, v)| (k.into(), v.into())).collect()))
}

#[pyfunction]
pub fn interpret_query(
    adapter: AdapterShim,
    schema: &Schema,
    query: &str,
    #[pyo3(from_py_with = "to_query_arguments")] arguments: Arc<
        BTreeMap<Arc<str>, TrustfallFieldValue>,
    >,
) -> PyResult<ResultIterator> {
    let wrapped_adapter = Arc::from(adapter);

    let indexed_query = parse(&schema.inner, query).map_err(|err| match err {
        FrontendError::ParseError(parse_err) => {
            crate::errors::ParseError::new_err(format!("{parse_err}"))
        }
        FrontendError::ValidationError(val_err) => {
            crate::errors::ValidationError::new_err(format!("{val_err}"))
        }
        _ => crate::errors::FrontendError::new_err(format!("{err}")),
    })?;

    let execution = interpret_ir(wrapped_adapter, indexed_query, arguments)
        .map_err(|err| crate::errors::QueryArgumentsError::new_err(format!("{err}")))?;
    let owned_iter: Box<dyn Iterator<Item = BTreeMap<String, Py<PyAny>>>> =
        Box::new(execution.map(|res| {
            res.iter()
                .map(|(k, v)| {
                    let py_value: FieldValue = v.clone().into();
                    Python::with_gil(|py| (k.to_string(), py_value.into_pyobject(py).expect("failed to convert FieldValue to Python object, this shouldn't be possible").unbind()))
                })
                .collect()
        }));

    Ok(ResultIterator { iter: owned_iter })
}

#[pyclass(unsendable)]
pub struct ResultIterator {
    iter: Box<dyn Iterator<Item = BTreeMap<String, Py<PyAny>>>>,
}

#[pymethods]
impl ResultIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<BTreeMap<String, Py<PyAny>>> {
        slf.iter.next()
    }
}

#[pyclass]
#[derive(Clone)]
pub struct AdapterShim {
    adapter: Arc<Py<PyAny>>,
}

#[pymethods]
impl AdapterShim {
    #[new]
    pub fn new(adapter: Py<PyAny>) -> Self {
        Self { adapter: Arc::new(adapter) }
    }
}

fn make_iterator<'py>(value: &Bound<'py, PyAny>, origin: &'static str) -> Bound<'py, PyIterator> {
    value.try_iter().unwrap_or_else(|e| panic!("{origin} is not an iterable (caused by {e})"))
}

#[pyclass(unsendable, frozen)]
#[derive(Debug, Clone)]
pub(crate) struct Opaque {
    data: *mut (),
    pub(crate) vertex: Option<Arc<Py<PyAny>>>,
}

impl Opaque {
    fn new<V: AsVertex<Arc<Py<PyAny>>> + 'static>(ctx: DataContext<V>) -> Self {
        let vertex = ctx.active_vertex::<Arc<Py<PyAny>>>().cloned();
        let boxed = Box::new(ctx);
        let data = Box::into_raw(boxed) as *mut ();

        Self { data, vertex }
    }

    /// Converts an `Opaque` into the `DataContext<V>` it points to.
    ///
    /// # Safety
    ///
    /// When an `Opaque` is constructed, it does not store the value of the `V` generic parameter
    /// it was constructed with. The caller of this function must ensure that the `V` parameter here
    /// is the same type as the one used in the `Opaque::new()` call that constructed `self` here.
    unsafe fn into_inner<V: AsVertex<Arc<Py<PyAny>>> + 'static>(self) -> DataContext<V> {
        // SAFETY: The caller of this `unsafe` method has made sure that the `V` parameter here
        //         matches the one in the `Opaque::new<V>()` call that constructed the `self` here.
        let boxed_ctx = unsafe { Box::from_raw(self.data as *mut DataContext<V>) };
        *boxed_ctx
    }
}

#[pymethods]
impl Opaque {
    #[getter]
    fn active_vertex(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        Ok(self.vertex.as_ref().map(|arc| (**arc).clone_ref(py)))
    }
}

#[pyclass(unsendable)]
pub struct ContextIterator(VertexIterator<'static, Opaque>);

impl ContextIterator {
    fn new<V: AsVertex<Arc<Py<PyAny>>> + 'static>(inner: BaseContextIterator<'static, V>) -> Self {
        Self(Box::new(inner.map(Opaque::new)))
    }
}

#[pymethods]
impl ContextIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<Opaque> {
        slf.0.next()
    }
}

impl Adapter<'static> for AdapterShim {
    type Vertex = Arc<Py<PyAny>>;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        _resolve_info: &ResolveInfo,
    ) -> VertexIterator<'static, Self::Vertex> {
        Python::with_gil(|py| {
            let parameter_data: BTreeMap<String, Py<PyAny>> = parameters
                .iter()
                .map(|(k, v)| {
                    (
                        k.to_string(),
                        FieldValue::from(v.clone())
                            .into_pyobject(py)
                            .expect("failed to convert FieldValue to Python object")
                            .unbind(),
                    )
                })
                .collect();

            let py_iterable = self
                .adapter
                .call_method(
                    py,
                    pyo3::intern!(py, "resolve_starting_vertices"),
                    (edge_name.as_ref(), parameter_data),
                    None,
                )
                .unwrap();

            let iter = make_iterator(py_iterable.bind(py), "resolve_starting_vertices()");
            Box::new(PythonVertexIterator::new(iter.unbind()))
        })
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'static>(
        &self,
        contexts: BaseContextIterator<'static, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'static, V, TrustfallFieldValue> {
        let contexts = ContextIterator::new(contexts);
        Python::with_gil(|py| {
            let py_iterable = self
                .adapter
                .call_method(
                    py,
                    pyo3::intern!(py, "resolve_property"),
                    (contexts, type_name.as_ref(), property_name.as_ref()),
                    None,
                )
                .unwrap();

            let iter = PythonResolvePropertyIterator::new(
                make_iterator(py_iterable.bind(py), "resolve_property()").unbind(),
            );

            Box::new(iter.map(|(opaque, value)| {
                // SAFETY: This `Opaque` was constructed just a few lines ago
                //         in this `resolve_property()` call, so the `V` type must be the same.
                let ctx = unsafe { opaque.into_inner() };

                (ctx, value.into())
            }))
        })
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'static>(
        &self,
        contexts: BaseContextIterator<'static, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'static, V, VertexIterator<'static, Self::Vertex>> {
        let contexts = ContextIterator::new(contexts);
        Python::with_gil(|py| {
            let parameter_data: BTreeMap<String, Py<PyAny>> = parameters
                .iter()
                .map(|(k, v)| {
                    (
                        k.to_string(),
                        FieldValue::from(v.clone())
                            .into_pyobject(py)
                            .expect("failed to convert FieldValue to Python object")
                            .unbind(),
                    )
                })
                .collect();

            let py_iterable = self
                .adapter
                .call_method(
                    py,
                    pyo3::intern!(py, "resolve_neighbors"),
                    (contexts, type_name.as_ref(), edge_name.as_ref(), parameter_data),
                    None,
                )
                .unwrap();

            let iter = PythonResolveNeighborsIterator::new(
                make_iterator(py_iterable.bind(py), "resolve_neighbors()").unbind(),
            );
            Box::new(iter.map(|(opaque, neighbors)| {
                // SAFETY: This `Opaque` was constructed just a few lines ago
                //         in this `resolve_neighbors()` call, so the `V` type must be the same.
                let ctx = unsafe { opaque.into_inner() };

                (ctx, neighbors)
            }))
        })
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'static>(
        &self,
        contexts: BaseContextIterator<'static, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'static, V, bool> {
        let contexts = ContextIterator::new(contexts);
        Python::with_gil(|py| {
            let py_iterable = self
                .adapter
                .call_method(
                    py,
                    pyo3::intern!(py, "resolve_coercion"),
                    (contexts, type_name.as_ref(), coerce_to_type.as_ref()),
                    None,
                )
                .unwrap();

            let iter = PythonResolveCoercionIterator::new(
                make_iterator(py_iterable.bind(py), "resolve_coercion()").unbind(),
            );
            Box::new(iter.map(|(opaque, value)| {
                // SAFETY: This `Opaque` was constructed just a few lines ago
                //         in this `resolve_coercion()` call, so the `V` type must be the same.
                let ctx = unsafe { opaque.into_inner() };

                (ctx, value)
            }))
        })
    }
}

struct PythonVertexIterator {
    underlying: Py<PyIterator>,
}

impl PythonVertexIterator {
    fn new(underlying: Py<PyIterator>) -> Self {
        Self { underlying }
    }
}

impl Iterator for PythonVertexIterator {
    type Item = Arc<Py<PyAny>>;

    fn next(&mut self) -> Option<Self::Item> {
        Python::with_gil(|py| match self.underlying.call_method(py, "__next__", (), None) {
            Ok(value) => Some(Arc::new(value)),
            Err(e) => {
                if e.is_instance_of::<PyStopIteration>(py) {
                    None
                } else {
                    println!("Got error: {e:?}");
                    e.print(py);
                    panic!();
                }
            }
        })
    }
}

struct PythonResolvePropertyIterator {
    underlying: Py<PyIterator>,
}

impl PythonResolvePropertyIterator {
    fn new(underlying: Py<PyIterator>) -> Self {
        Self { underlying }
    }
}

/// Nicer error messages on `.expect()`-like calls in Python.
///
/// Without this trait, Rust errors triggered by `.expect()` on Python error look like this:
/// ```text
/// resolve_property() tuple element at index 1 is not a property value: PyErr { type: \
/// <class 'ValueError'>, value: ValueError("Value <object object at 0x7fed6dbd0eb0> \
/// of type <class 'object'> is not supported by Trustfall"), traceback: None }
/// ```
///
/// We'd like the error to appear like this:
/// ```text
/// resolve_property() tuple element at index 1 is not a property value (caused by ValueError: \
/// Value <object object at 0x7fed6dbd0eb0> of type <class 'object'> is not supported by Trustfall)
/// ```
trait ExpectPython {
    type Outcome;

    fn py_friendly_expect(self, msg: &str) -> Self::Outcome;
}

impl<T, E: std::fmt::Display> ExpectPython for Result<T, E> {
    type Outcome = T;

    fn py_friendly_expect(self, msg: &str) -> Self::Outcome {
        match self {
            Ok(v) => v,
            Err(e) => panic!("{msg} (caused by {e})"),
        }
    }
}

impl Iterator for PythonResolvePropertyIterator {
    type Item = (Opaque, FieldValue);

    fn next(&mut self) -> Option<Self::Item> {
        Python::with_gil(|py| {
            match self.underlying.call_method0(py, pyo3::intern!(py, "__next__")) {
                Ok(output) => {
                    // `output` must be a (context, property_value) tuple here, or else we panic.
                    let tuple = output.downcast_bound(py).py_friendly_expect(
                        "resolve_property() did not yield a `(context, property_value)` tuple",
                    );

                    let tuple_size_error: &'static str =
                        "resolve_property() yielded a tuple that did not have exactly 2 elements";

                    let property_value: FieldValue = tuple
                        .get_borrowed_item(1)
                        .py_friendly_expect(tuple_size_error)
                        .extract()
                        .py_friendly_expect(
                            "resolve_property() tuple element at index 1 is not a property value",
                        );

                    let context: Opaque = tuple.get_borrowed_item(0)
                        .py_friendly_expect(tuple_size_error)
                        .extract()
                        .py_friendly_expect("resolve_property() tuple element at index 0 is not a context (Opaque) value");

                    Some((context, property_value))
                }
                Err(e) => {
                    if e.is_instance_of::<PyStopIteration>(py) {
                        None
                    } else {
                        println!("Got error: {e:?}");
                        e.print(py);
                        panic!();
                    }
                }
            }
        })
    }
}

struct PythonResolveNeighborsIterator {
    underlying: Py<PyIterator>,
}

impl PythonResolveNeighborsIterator {
    fn new(underlying: Py<PyIterator>) -> Self {
        Self { underlying }
    }
}

impl Iterator for PythonResolveNeighborsIterator {
    type Item = (Opaque, VertexIterator<'static, Arc<Py<PyAny>>>);

    fn next(&mut self) -> Option<Self::Item> {
        Python::with_gil(|py| {
            match self.underlying.call_method0(py, pyo3::intern!(py, "__next__")) {
                Ok(output) => {
                    // `output` must be a (context, neighbor_iterator) tuple here, or else we panic.
                    let tuple: &Bound<'_, PyTuple> = output.downcast_bound(py).py_friendly_expect(
                        "resolve_neighbors() did not yield a `(context, neighbor_iterator)` tuple",
                    );

                    let tuple_size_error: &'static str =
                        "resolve_neighbors() yielded a tuple that did not have exactly 2 elements";

                    let neighbors_iterable =
                        tuple.get_borrowed_item(1).py_friendly_expect(tuple_size_error);

                    let context: Opaque = tuple.get_borrowed_item(0)
                        .py_friendly_expect(tuple_size_error)
                        .extract()
                        .py_friendly_expect("resolve_neighbors() tuple element at index 0 is not a context (Opaque) value");

                    // Support returning iterables (e.g. []), not just iterators.
                    // Iterators return self when `__iter__()` is called.
                    let neighbors_iter = make_iterator(
                        &neighbors_iterable,
                        "resolve_neighbors() yielded tuple's second element",
                    );

                    let neighbors: VertexIterator<'static, Arc<Py<PyAny>>> =
                        Box::new(PythonVertexIterator::new(neighbors_iter.unbind()));
                    Some((context, neighbors))
                }
                Err(e) => {
                    if e.is_instance_of::<PyStopIteration>(py) {
                        None
                    } else {
                        println!("Got error: {e:?}");
                        e.print(py);
                        panic!();
                    }
                }
            }
        })
    }
}

struct PythonResolveCoercionIterator {
    underlying: Py<PyIterator>,
}

impl PythonResolveCoercionIterator {
    fn new(underlying: Py<PyIterator>) -> Self {
        Self { underlying }
    }
}

impl Iterator for PythonResolveCoercionIterator {
    type Item = (Opaque, bool);

    fn next(&mut self) -> Option<Self::Item> {
        Python::with_gil(|py| {
            match self.underlying.call_method0(py, pyo3::intern!(py, "__next__")) {
                Ok(output) => {
                    // `output` must be a (context, can_coerce) tuple here, or else we panic.
                    let tuple = output.downcast_bound(py).py_friendly_expect(
                        "resolve_coercion() did not yield a `(context, can_coerce)` tuple",
                    );

                    let tuple_size_error: &'static str =
                        "resolve_coercion() yielded a tuple that did not have exactly 2 elements";

                    let can_coerce: bool = tuple
                        .get_borrowed_item(1)
                        .py_friendly_expect(tuple_size_error)
                        .extract()
                        .py_friendly_expect(
                            "resolve_coercion() tuple element at index 1 is not a bool",
                        );

                    let context: Opaque = tuple.get_borrowed_item(0)
                        .py_friendly_expect(tuple_size_error)
                        .extract()
                        .py_friendly_expect("resolve_coercion() tuple element at index 0 is not a context (Opaque) value");

                    Some((context, can_coerce))
                }
                Err(e) => {
                    if e.is_instance_of::<PyStopIteration>(py) {
                        None
                    } else {
                        println!("Got error: {e:?}");
                        e.print(py);
                        panic!();
                    }
                }
            }
        })
    }
}
