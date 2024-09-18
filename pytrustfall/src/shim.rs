use std::{collections::BTreeMap, sync::Arc};

use pyo3::{exceptions::PyStopIteration, prelude::*, wrap_pyfunction};
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

#[pyclass]
pub struct Schema {
    inner: trustfall_core::schema::Schema,
}

#[pymethods]
impl Schema {
    #[new]
    pub fn new(schema_text: &str) -> PyResult<Self> {
        let inner = trustfall_core::schema::Schema::parse(schema_text).map_err(|e| {
            Python::with_gil(|py| {
                crate::errors::InvalidSchemaError::new_err(format!("{e}").into_py(py))
            })
        })?;

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
        FrontendError::ParseError(parse_err) => Python::with_gil(|py| {
            crate::errors::ParseError::new_err(format!("{parse_err}").into_py(py))
        }),
        FrontendError::ValidationError(val_err) => Python::with_gil(|py| {
            crate::errors::ValidationError::new_err(format!("{val_err}").into_py(py))
        }),
        _ => Python::with_gil(|py| {
            crate::errors::FrontendError::new_err(format!("{err}").into_py(py))
        }),
    })?;

    let execution = interpret_ir(wrapped_adapter, indexed_query, arguments).map_err(|err| {
        Python::with_gil(|py| {
            crate::errors::QueryArgumentsError::new_err(format!("{err}").into_py(py))
        })
    })?;
    let owned_iter: Box<dyn Iterator<Item = BTreeMap<String, Py<PyAny>>>> =
        Box::new(execution.map(|res| {
            res.iter()
                .map(|(k, v)| {
                    let py_value: FieldValue = v.clone().into();
                    Python::with_gil(|py| {
                        let python_value = make_python_value(py, py_value);
                        (k.to_string(), python_value)
                    })
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

// TODO: this is just `IntoPy`: https://pyo3.rs/v0.22.3/conversions/traits#intopyt
fn make_python_value(py: Python<'_>, value: FieldValue) -> Py<PyAny> {
    match value {
        FieldValue::Null => Option::<i64>::None.into_py(py),
        FieldValue::Uint64(x) => x.into_py(py),
        FieldValue::Int64(x) => x.into_py(py),
        FieldValue::Float64(x) => x.into_py(py),
        FieldValue::String(x) => x.into_py(py),
        FieldValue::Boolean(x) => x.into_py(py),
        FieldValue::Enum(_) => todo!(),
        FieldValue::List(x) => {
            x.into_iter().map(|v| make_python_value(py, v)).collect::<Vec<_>>().into_py(py)
        }
    }
}

fn make_iterator<'py>(value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    value.call_method0("__iter__")
}

#[pyclass(unsendable)]
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
                .map(|(k, v)| (k.to_string(), make_python_value(py, v.clone().into())))
                .collect();

            // TODO: use `intern!()` macro to intern the fixed method names for efficiency
            let py_iterable = self
                .adapter
                .call_method_bound(
                    py,
                    "resolve_starting_vertices",
                    (edge_name.as_ref(), parameter_data),
                    None,
                )
                .unwrap();
            let iter = make_iterator(py_iterable.bind(py)).unwrap();
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
                .call_method_bound(
                    py,
                    pyo3::intern!(py, "resolve_property"),
                    (contexts, type_name.as_ref(), property_name.as_ref()),
                    None,
                )
                .unwrap();

            let iter = PythonResolvePropertyIterator::new(
                make_iterator(py_iterable.bind(py))
                    .expect("failed to use py_iterable as an iterator")
                    .unbind(),
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
                .map(|(k, v)| (k.to_string(), make_python_value(py, v.clone().into())))
                .collect();

            let py_iterable = self
                .adapter
                .call_method_bound(
                    py,
                    pyo3::intern!(py, "resolve_neighbors"),
                    (contexts, type_name.as_ref(), edge_name.as_ref(), parameter_data),
                    None,
                )
                .unwrap();

            let iter = PythonResolveNeighborsIterator::new(
                make_iterator(py_iterable.bind(py))
                    .expect("failed to use py_iterable as an iterator")
                    .unbind(),
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
                .call_method_bound(
                    py,
                    pyo3::intern!(py, "resolve_coercion"),
                    (contexts, type_name.as_ref(), coerce_to_type.as_ref()),
                    None,
                )
                .unwrap();

            let iter = PythonResolveCoercionIterator::new(
                make_iterator(py_iterable.bind(py))
                    .expect("failed to use py_iterable as an iterator")
                    .unbind(),
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
    underlying: Py<PyAny>,
}

impl PythonVertexIterator {
    fn new(underlying: Py<PyAny>) -> Self {
        Self { underlying }
    }
}

impl Iterator for PythonVertexIterator {
    type Item = Arc<Py<PyAny>>;

    fn next(&mut self) -> Option<Self::Item> {
        Python::with_gil(|py| match self.underlying.call_method_bound(py, "__next__", (), None) {
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
    underlying: Py<PyAny>,
}

impl PythonResolvePropertyIterator {
    fn new(underlying: Py<PyAny>) -> Self {
        Self { underlying }
    }
}

impl Iterator for PythonResolvePropertyIterator {
    type Item = (Opaque, FieldValue);

    fn next(&mut self) -> Option<Self::Item> {
        Python::with_gil(|py| {
            match self.underlying.call_method_bound(py, "__next__", (), None) {
                Ok(output) => {
                    // value is a (context, property_value) tuple here
                    let context: Opaque = output
                        .call_method_bound(py, "__getitem__", (0i64,), None)
                        .unwrap()
                        .extract(py)
                        .unwrap();

                    // TODO: if this panics, we got an unrepresentable FieldValue,
                    //       which should be a proper error
                    let value: FieldValue = output
                        .call_method_bound(py, "__getitem__", (1i64,), None)
                        .unwrap()
                        .extract(py)
                        .unwrap();

                    Some((context, value))
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
    underlying: Py<PyAny>,
}

impl PythonResolveNeighborsIterator {
    fn new(underlying: Py<PyAny>) -> Self {
        Self { underlying }
    }
}

impl Iterator for PythonResolveNeighborsIterator {
    type Item = (Opaque, VertexIterator<'static, Arc<Py<PyAny>>>);

    fn next(&mut self) -> Option<Self::Item> {
        Python::with_gil(|py| {
            match self.underlying.call_method_bound(py, "__next__", (), None) {
                Ok(output) => {
                    // value is a (context, neighbor_iterator) tuple here
                    let context: Opaque = output
                        .call_method_bound(py, "__getitem__", (0i64,), None)
                        .unwrap()
                        .extract(py)
                        .unwrap();
                    let neighbors_iterable =
                        output.call_method_bound(py, "__getitem__", (1i64,), None).unwrap();

                    // Allow returning iterables (e.g. []), not just iterators.
                    // Iterators return self when __iter__() is called.
                    let neighbors_iter = make_iterator(neighbors_iterable.bind(py)).unwrap();

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
    underlying: Py<PyAny>,
}

impl PythonResolveCoercionIterator {
    fn new(underlying: Py<PyAny>) -> Self {
        Self { underlying }
    }
}

impl Iterator for PythonResolveCoercionIterator {
    type Item = (Opaque, bool);

    fn next(&mut self) -> Option<Self::Item> {
        Python::with_gil(|py| {
            match self.underlying.call_method_bound(py, "__next__", (), None) {
                Ok(output) => {
                    // value is a (context, can_coerce) tuple here
                    let context: Opaque = output
                        .call_method_bound(py, "__getitem__", (0i64,), None)
                        .unwrap()
                        .extract(py)
                        .unwrap();
                    let can_coerce: bool = output
                        .call_method_bound(py, "__getitem__", (1i64,), None)
                        .unwrap()
                        .extract::<bool>(py)
                        .unwrap();
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
