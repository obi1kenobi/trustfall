#![allow(unused_imports)]

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    fs,
    rc::Rc,
    sync::Arc,
};

use pyo3::{
    exceptions::PyStopIteration, prelude::*, types::PyTuple, wrap_pyfunction, PyIterProtocol,
};

use trustfall_core::{
    frontend::{error::FrontendError, parse},
    interpreter::{execution::interpret_ir, Adapter, DataContext, InterpretedQuery},
    ir::{EdgeParameters, Eid, FieldValue, Vid},
};

use crate::errors::QueryArgumentsError;

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
    pub fn new(schema_text: &str) -> Self {
        Self {
            inner: trustfall_core::schema::Schema::parse(schema_text).unwrap(),
        }
    }
}

fn to_query_arguments(src: &PyAny) -> PyResult<Arc<HashMap<Arc<str>, FieldValue>>> {
    let args = src.extract::<HashMap<String, &PyAny>>()?;

    let mut unrepresentable_args = vec![];
    let mut converted_args = HashMap::with_capacity(args.len());

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
                    into a representation usable by the query engine: {:?}",
                    unrepresentable_args
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
    #[pyo3(from_py_with = "to_query_arguments")] arguments: Arc<HashMap<Arc<str>, FieldValue>>,
) -> PyResult<ResultIterator> {
    let wrapped_adapter = Rc::new(RefCell::new(adapter));

    let indexed_query = parse(&schema.inner, query).map_err(|err| match err {
        FrontendError::ParseError(parse_err) => Python::with_gil(|py| {
            crate::errors::ParseError::new_err(format!("{}", parse_err).into_py(py))
        }),
        FrontendError::ValidationError(val_err) => Python::with_gil(|py| {
            crate::errors::ValidationError::new_err(format!("{}", val_err).into_py(py))
        }),
        _ => Python::with_gil(|py| {
            crate::errors::FrontendError::new_err(format!("{}", err).into_py(py))
        }),
    })?;

    let execution = interpret_ir(wrapped_adapter, indexed_query, arguments).map_err(|err| {
        Python::with_gil(|py| {
            crate::errors::QueryArgumentsError::new_err(format!("{}", err).into_py(py))
        })
    })?;
    let owned_iter: Box<dyn Iterator<Item = HashMap<String, Py<PyAny>>>> =
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
    iter: Box<dyn Iterator<Item = HashMap<String, Py<PyAny>>>>,
}

#[pyproto]
impl PyIterProtocol for ResultIterator {
    fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<Self>) -> Option<HashMap<String, Py<PyAny>>> {
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
    fn current_token(&self) -> PyResult<Option<Py<PyAny>>> {
        Ok(self.0.current_token.as_ref().map(|arc| (**arc).clone()))
    }
}

#[pyclass(unsendable)]
pub struct ContextIterator(Box<dyn Iterator<Item = Context>>);

impl ContextIterator {
    fn new(inner: Box<dyn Iterator<Item = DataContext<Arc<Py<PyAny>>>>>) -> Self {
        Self(Box::new(inner.map(Context)))
    }
}

#[pyproto]
impl PyIterProtocol for ContextIterator {
    fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<Self>) -> Option<Context> {
        slf.0.next()
    }
}

#[allow(unused_variables)]
impl Adapter<'static> for AdapterShim {
    type DataToken = Arc<Py<PyAny>>;

    fn get_starting_tokens(
        &mut self,
        edge: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken>> {
        Python::with_gil(|py| {
            let parameter_data: Option<BTreeMap<String, Py<PyAny>>> = parameters.map(|x| {
                x.0.iter()
                    .map(|(k, v)| (k.to_string(), make_python_value(py, v.to_owned())))
                    .collect()
            });

            let py_iter = self
                .adapter
                .call_method(
                    py,
                    "get_starting_tokens",
                    (edge.as_ref(), parameter_data),
                    None,
                )
                .unwrap();
            Box::new(PythonTokenIterator::new(py_iter))
        })
    }

    fn project_property(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>>>,
        current_type_name: Arc<str>,
        field_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, FieldValue)>> {
        let contexts = ContextIterator::new(data_contexts);
        Python::with_gil(|py| {
            let py_iterable = self
                .adapter
                .call_method(
                    py,
                    "project_property",
                    (contexts, current_type_name.as_ref(), field_name.as_ref()),
                    None,
                )
                .unwrap();

            let iter = make_iterator(py, py_iterable).unwrap();
            Box::new(PythonProjectPropertyIterator::new(iter))
        })
    }

    #[allow(clippy::type_complexity)]
    fn project_neighbors(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>>>,
        current_type_name: Arc<str>,
        edge_name: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
        edge_hint: Eid,
    ) -> Box<
        dyn Iterator<
            Item = (
                DataContext<Self::DataToken>,
                Box<dyn Iterator<Item = Self::DataToken>>,
            ),
        >,
    > {
        let contexts = ContextIterator::new(data_contexts);
        Python::with_gil(|py| {
            let parameter_data: Option<BTreeMap<String, Py<PyAny>>> = parameters.map(|x| {
                x.0.iter()
                    .map(|(k, v)| (k.to_string(), make_python_value(py, v.to_owned())))
                    .collect()
            });

            let py_iterable = self
                .adapter
                .call_method(
                    py,
                    "project_neighbors",
                    (
                        contexts,
                        current_type_name.as_ref(),
                        edge_name.as_ref(),
                        parameter_data,
                    ),
                    None,
                )
                .unwrap();

            let iter = make_iterator(py, py_iterable).unwrap();
            Box::new(PythonProjectNeighborsIterator::new(iter))
        })
    }

    fn can_coerce_to_type(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>>>,
        current_type_name: Arc<str>,
        coerce_to_type_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, bool)>> {
        let contexts = ContextIterator::new(data_contexts);
        Python::with_gil(|py| {
            let py_iterable = self
                .adapter
                .call_method(
                    py,
                    "can_coerce_to_type",
                    (
                        contexts,
                        current_type_name.as_ref(),
                        coerce_to_type_name.as_ref(),
                    ),
                    None,
                )
                .unwrap();

            let iter = make_iterator(py, py_iterable).unwrap();
            Box::new(PythonCanCoerceToTypeIterator::new(iter))
        })
    }
}

struct PythonTokenIterator {
    underlying: Py<PyAny>,
}

impl PythonTokenIterator {
    fn new(underlying: Py<PyAny>) -> Self {
        Self { underlying }
    }
}

impl Iterator for PythonTokenIterator {
    type Item = Arc<Py<PyAny>>;

    fn next(&mut self) -> Option<Self::Item> {
        Python::with_gil(
            |py| match self.underlying.call_method(py, "__next__", (), None) {
                Ok(value) => Some(Arc::new(value)),
                Err(e) => {
                    if e.is_instance::<PyStopIteration>(py) {
                        None
                    } else {
                        println!("Got error: {:?}", e);
                        e.print(py);
                        panic!();
                    }
                }
            },
        )
    }
}

struct PythonProjectPropertyIterator {
    underlying: Py<PyAny>,
}

impl PythonProjectPropertyIterator {
    fn new(underlying: Py<PyAny>) -> Self {
        Self { underlying }
    }
}

impl Iterator for PythonProjectPropertyIterator {
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
                    if e.is_instance::<PyStopIteration>(py) {
                        None
                    } else {
                        println!("Got error: {:?}", e);
                        e.print(py);
                        panic!();
                    }
                }
            }
        })
    }
}

struct PythonProjectNeighborsIterator {
    underlying: Py<PyAny>,
}

impl PythonProjectNeighborsIterator {
    fn new(underlying: Py<PyAny>) -> Self {
        Self { underlying }
    }
}

impl Iterator for PythonProjectNeighborsIterator {
    #[allow(clippy::type_complexity)]
    type Item = (
        DataContext<Arc<Py<PyAny>>>,
        Box<dyn Iterator<Item = Arc<Py<PyAny>>>>,
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

                    let neighbors: Box<dyn Iterator<Item = Arc<Py<PyAny>>>> =
                        Box::new(PythonTokenIterator::new(neighbors_iter));
                    Some((context.0, neighbors))
                }
                Err(e) => {
                    if e.is_instance::<PyStopIteration>(py) {
                        None
                    } else {
                        println!("Got error: {:?}", e);
                        e.print(py);
                        panic!();
                    }
                }
            }
        })
    }
}

struct PythonCanCoerceToTypeIterator {
    underlying: Py<PyAny>,
}

impl PythonCanCoerceToTypeIterator {
    fn new(underlying: Py<PyAny>) -> Self {
        Self { underlying }
    }
}

impl Iterator for PythonCanCoerceToTypeIterator {
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
                    if e.is_instance::<PyStopIteration>(py) {
                        None
                    } else {
                        println!("Got error: {:?}", e);
                        e.print(py);
                        panic!();
                    }
                }
            }
        })
    }
}
