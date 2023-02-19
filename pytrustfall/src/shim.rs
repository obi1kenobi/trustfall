use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

use pyo3::{exceptions::PyStopIteration, prelude::*, wrap_pyfunction};

use trustfall_core::{
    frontend::{error::FrontendError, parse},
    interpreter::{
        execution::interpret_ir, Adapter, ContextIterator as BaseContextIterator,
        ContextOutcomeIterator, DataContext, QueryInfo, VertexIterator,
    },
    ir::{EdgeParameters, FieldValue},
};

pub(crate) fn register(_py: Python, m: &PyModule) -> PyResult<()> {
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

fn to_query_arguments(src: &PyAny) -> PyResult<Arc<BTreeMap<Arc<str>, FieldValue>>> {
    let args = src.extract::<BTreeMap<String, &PyAny>>()?;

    let mut unrepresentable_args = vec![];
    let mut converted_args = BTreeMap::new();

    for (arg_name, arg_value) in args {
        match make_field_value_from_ref(arg_value) {
            Ok(value) => {
                let inserted = converted_args.insert(Arc::from(arg_name), value);
                assert!(inserted.is_none());
            }
            Err(_) => {
                unrepresentable_args.push(arg_name);
            }
        }
    }

    if unrepresentable_args.is_empty() {
        Ok(Arc::from(converted_args))
    } else {
        Err(Python::with_gil(|py| {
            crate::errors::QueryArgumentsError::new_err(
                format!(
                    "Encountered argument(s) with unexpected types that could not be converted \
                    into a representation usable by the query engine: {unrepresentable_args:?}",
                )
                .into_py(py),
            )
        }))
    }
}

#[pyfunction]
pub fn interpret_query(
    adapter: AdapterShim,
    schema: &Schema,
    query: &str,
    #[pyo3(from_py_with = "to_query_arguments")] arguments: Arc<BTreeMap<Arc<str>, FieldValue>>,
) -> PyResult<ResultIterator> {
    let wrapped_adapter = Rc::new(RefCell::new(adapter));

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
            res.into_iter()
                .map(|(k, v)| {
                    Python::with_gil(|py| {
                        let python_value = make_python_value(py, v);
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
    fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<Self>) -> Option<BTreeMap<String, Py<PyAny>>> {
        slf.iter.next()
    }
}

#[pyclass]
#[derive(Clone)]
pub struct AdapterShim {
    adapter: Py<PyAny>,
}

#[pymethods]
impl AdapterShim {
    #[new]
    pub fn new(adapter: Py<PyAny>) -> Self {
        AdapterShim { adapter }
    }
}

fn make_python_value(py: Python, value: FieldValue) -> Py<PyAny> {
    match value {
        FieldValue::Null => Option::<i64>::None.into_py(py),
        FieldValue::Uint64(x) => x.into_py(py),
        FieldValue::Int64(x) => x.into_py(py),
        FieldValue::Float64(x) => x.into_py(py),
        FieldValue::String(x) => x.into_py(py),
        FieldValue::Boolean(x) => x.into_py(py),
        FieldValue::DateTimeUtc(_) => todo!(),
        FieldValue::Enum(_) => todo!(),
        FieldValue::List(x) => x
            .into_iter()
            .map(|v| make_python_value(py, v))
            .collect::<Vec<_>>()
            .into_py(py),
    }
}

fn make_field_value_from_ref(value: &PyAny) -> Result<FieldValue, ()> {
    if value.is_none() {
        Ok(FieldValue::Null)
    } else if let Ok(inner) = value.extract::<bool>() {
        Ok(FieldValue::Boolean(inner))
    } else if let Ok(inner) = value.extract::<i64>() {
        Ok(FieldValue::Int64(inner))
    } else if let Ok(inner) = value.extract::<f64>() {
        Ok(FieldValue::Float64(inner))
    } else if let Ok(inner) = value.extract::<String>() {
        Ok(FieldValue::String(inner))
    } else if let Ok(inner) = value.extract::<Vec<&PyAny>>() {
        let converted_values = inner
            .iter()
            .copied()
            .map(make_field_value_from_ref)
            .try_fold(vec![], |mut acc, item| {
                if let Ok(value) = item {
                    acc.push(value);
                    Some(acc)
                } else {
                    None
                }
            });

        if let Some(inner_values) = converted_values {
            Ok(FieldValue::List(inner_values))
        } else {
            Err(())
        }
    } else {
        Err(())
    }
}

fn make_iterator(py: Python, value: Py<PyAny>) -> PyResult<Py<PyAny>> {
    value.call_method(py, "__iter__", (), None)
}

#[pyclass]
#[derive(Debug, Clone)]
pub struct Context(DataContext<Arc<Py<PyAny>>>);

#[pymethods]
impl Context {
    #[getter]
    fn active_vertex(&self) -> PyResult<Option<Py<PyAny>>> {
        Ok(self.0.active_vertex().map(|arc| (**arc).clone()))
    }
}

#[pyclass(unsendable)]
pub struct ContextIterator(VertexIterator<'static, Context>);

impl ContextIterator {
    fn new(inner: VertexIterator<'static, DataContext<Arc<Py<PyAny>>>>) -> Self {
        Self(Box::new(inner.map(Context)))
    }
}

#[pymethods]
impl ContextIterator {
    fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<Self>) -> Option<Context> {
        slf.0.next()
    }
}

impl Adapter<'static> for AdapterShim {
    type Vertex = Arc<Py<PyAny>>;

    fn resolve_starting_vertices(
        &mut self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        _query_info: &QueryInfo,
    ) -> VertexIterator<'static, Self::Vertex> {
        Python::with_gil(|py| {
            let parameter_data: BTreeMap<String, Py<PyAny>> = parameters
                .iter()
                .map(|(k, v)| (k.to_string(), make_python_value(py, v.to_owned())))
                .collect();

            let py_iterable = self
                .adapter
                .call_method(
                    py,
                    "resolve_starting_vertices",
                    (edge_name.as_ref(), parameter_data),
                    None,
                )
                .unwrap();
            let iter = make_iterator(py, py_iterable).unwrap();
            Box::new(PythonVertexIterator::new(iter))
        })
    }

    fn resolve_property(
        &mut self,
        contexts: BaseContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        _query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
        let contexts = ContextIterator::new(contexts);
        Python::with_gil(|py| {
            let py_iterable = self
                .adapter
                .call_method(
                    py,
                    "resolve_property",
                    (contexts, type_name.as_ref(), property_name.as_ref()),
                    None,
                )
                .unwrap();

            let iter = make_iterator(py, py_iterable).unwrap();
            Box::new(PythonResolvePropertyIterator::new(iter))
        })
    }

    fn resolve_neighbors(
        &mut self,
        contexts: BaseContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        _query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>> {
        let contexts = ContextIterator::new(contexts);
        Python::with_gil(|py| {
            let parameter_data: BTreeMap<String, Py<PyAny>> = parameters
                .iter()
                .map(|(k, v)| (k.to_string(), make_python_value(py, v.to_owned())))
                .collect();

            let py_iterable = self
                .adapter
                .call_method(
                    py,
                    "resolve_neighbors",
                    (
                        contexts,
                        type_name.as_ref(),
                        edge_name.as_ref(),
                        parameter_data,
                    ),
                    None,
                )
                .unwrap();

            let iter = make_iterator(py, py_iterable).unwrap();
            Box::new(PythonResolveNeighborsIterator::new(iter))
        })
    }

    fn resolve_coercion(
        &mut self,
        contexts: BaseContextIterator<'static, Self::Vertex>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        _query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
        let contexts = ContextIterator::new(contexts);
        Python::with_gil(|py| {
            let py_iterable = self
                .adapter
                .call_method(
                    py,
                    "resolve_coercion",
                    (contexts, type_name.as_ref(), coerce_to_type.as_ref()),
                    None,
                )
                .unwrap();

            let iter = make_iterator(py, py_iterable).unwrap();
            Box::new(PythonResolveCoercionIterator::new(iter))
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
        Python::with_gil(
            |py| match self.underlying.call_method(py, "__next__", (), None) {
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
            },
        )
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
    type Item = (DataContext<Arc<Py<PyAny>>>, FieldValue);

    fn next(&mut self) -> Option<Self::Item> {
        Python::with_gil(|py| {
            match self.underlying.call_method(py, "__next__", (), None) {
                Ok(output) => {
                    // value is a (context, property_value) tuple here
                    let context: Context = output
                        .call_method(py, "__getitem__", (0i64,), None)
                        .unwrap()
                        .extract(py)
                        .unwrap();

                    // TODO: if this panics, we got an unrepresentable FieldValue,
                    //       which should be a proper error
                    let value: FieldValue = make_field_value_from_ref(
                        output
                            .call_method(py, "__getitem__", (1i64,), None)
                            .unwrap()
                            .as_ref(py),
                    )
                    .unwrap();

                    Some((context.0, value))
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
    type Item = (
        DataContext<Arc<Py<PyAny>>>,
        VertexIterator<'static, Arc<Py<PyAny>>>,
    );

    fn next(&mut self) -> Option<Self::Item> {
        Python::with_gil(|py| {
            match self.underlying.call_method(py, "__next__", (), None) {
                Ok(output) => {
                    // value is a (context, neighbor_iterator) tuple here
                    let context: Context = output
                        .call_method(py, "__getitem__", (0i64,), None)
                        .unwrap()
                        .extract(py)
                        .unwrap();
                    let neighbors_iterable = output
                        .call_method(py, "__getitem__", (1i64,), None)
                        .unwrap();

                    // Allow returning iterables (e.g. []), not just iterators.
                    // Iterators return self when __iter__() is called.
                    let neighbors_iter = make_iterator(py, neighbors_iterable).unwrap();

                    let neighbors: VertexIterator<'static, Arc<Py<PyAny>>> =
                        Box::new(PythonVertexIterator::new(neighbors_iter));
                    Some((context.0, neighbors))
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
    type Item = (DataContext<Arc<Py<PyAny>>>, bool);

    fn next(&mut self) -> Option<Self::Item> {
        Python::with_gil(|py| {
            match self.underlying.call_method(py, "__next__", (), None) {
                Ok(output) => {
                    // value is a (context, can_coerce) tuple here
                    let context: Context = output
                        .call_method(py, "__getitem__", (0i64,), None)
                        .unwrap()
                        .extract(py)
                        .unwrap();
                    let can_coerce: bool = output
                        .call_method(py, "__getitem__", (1i64,), None)
                        .unwrap()
                        .extract::<bool>(py)
                        .unwrap();
                    Some((context.0, can_coerce))
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
