#![allow(dead_code)]

use std::fs::{self, ReadDir};
use std::iter;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::interpreter::AsVertex;
use crate::{
    interpreter::{
        Adapter, ContextIterator, ContextOutcomeIterator, DataContext, ResolveEdgeInfo,
        ResolveInfo, VertexIterator,
    },
    ir::{EdgeParameters, FieldValue},
};

#[derive(Debug, Clone)]
pub struct FilesystemInterpreter {
    origin: Rc<PathBuf>,
}

impl FilesystemInterpreter {
    pub fn new(origin: PathBuf) -> FilesystemInterpreter {
        FilesystemInterpreter { origin: Rc::new(origin) }
    }
}

#[derive(Debug)]
struct OriginIterator {
    origin_vertex: DirectoryVertex,
    produced: bool,
}

impl OriginIterator {
    pub fn new(vertex: DirectoryVertex) -> OriginIterator {
        OriginIterator { origin_vertex: vertex, produced: false }
    }
}

impl Iterator for OriginIterator {
    type Item = FilesystemVertex;

    fn next(&mut self) -> Option<FilesystemVertex> {
        if self.produced {
            None
        } else {
            self.produced = true;
            Some(FilesystemVertex::Directory(self.origin_vertex.clone()))
        }
    }
}

#[derive(Debug)]
struct DirectoryContainsFileIterator {
    origin: Rc<PathBuf>,
    directory: DirectoryVertex,
    file_iter: Option<ReadDir>,
}

impl DirectoryContainsFileIterator {
    pub fn new(origin: Rc<PathBuf>, directory: &DirectoryVertex) -> DirectoryContainsFileIterator {
        let buf = origin.join(&directory.path);
        DirectoryContainsFileIterator {
            origin,
            directory: directory.clone(),
            file_iter: fs::read_dir(buf).ok(),
        }
    }
}

impl Iterator for DirectoryContainsFileIterator {
    type Item = FilesystemVertex;

    fn next(&mut self) -> Option<FilesystemVertex> {
        let file_iter = self.file_iter.as_mut()?;
        loop {
            match file_iter.next()? {
                Ok(dir_entry) => {
                    let metadata = match dir_entry.metadata() {
                        Ok(res) => res,
                        _ => continue,
                    };
                    if metadata.is_file() {
                        let name = dir_entry.file_name().to_string_lossy().into_owned();
                        let extension =
                            Path::new(&name).extension().map(|x| x.to_string_lossy().into_owned());
                        let path = join_with_slash(&self.directory.path, &name);
                        let result = FileVertex { name, extension, path };
                        return Some(FilesystemVertex::File(result));
                    }
                }
                _ => continue,
            }
        }
    }
}

#[derive(Debug)]
struct SubdirectoryIterator {
    origin: Rc<PathBuf>,
    directory: DirectoryVertex,
    dir_iter: Option<ReadDir>,
}

impl SubdirectoryIterator {
    pub fn new(origin: Rc<PathBuf>, directory: &DirectoryVertex) -> Self {
        let buf = origin.join(&directory.path);
        Self { origin, directory: directory.clone(), dir_iter: fs::read_dir(buf).ok() }
    }
}

impl Iterator for SubdirectoryIterator {
    type Item = FilesystemVertex;

    fn next(&mut self) -> Option<FilesystemVertex> {
        let dir_iter = self.dir_iter.as_mut()?;
        loop {
            match dir_iter.next()? {
                Ok(dir_entry) => {
                    let metadata = match dir_entry.metadata() {
                        Ok(res) => res,
                        _ => continue,
                    };
                    if metadata.is_dir() {
                        let name = dir_entry.file_name().to_string_lossy().into_owned();
                        if name == ".git" || name == ".vscode" || name == "target" {
                            continue;
                        }

                        let path = join_with_slash(&self.directory.path, &name);
                        let result = DirectoryVertex { name, path };
                        return Some(FilesystemVertex::Directory(result));
                    }
                }
                _ => continue,
            }
        }
    }
}

pub type ContextAndValue = (DataContext<FilesystemVertex>, FieldValue);

type IndividualEdgeResolver<'a> =
    fn(Rc<PathBuf>, &FilesystemVertex) -> VertexIterator<'a, FilesystemVertex>;
type ContextAndIterableOfEdges<'a, V> = (DataContext<V>, VertexIterator<'a, FilesystemVertex>);

struct EdgeResolverIterator<'a, V: AsVertex<FilesystemVertex>> {
    origin: Rc<PathBuf>,
    contexts: VertexIterator<'a, DataContext<V>>,
    edge_resolver: IndividualEdgeResolver<'a>,
}

impl<'a, V: AsVertex<FilesystemVertex>> EdgeResolverIterator<'a, V> {
    pub fn new(
        origin: Rc<PathBuf>,
        contexts: VertexIterator<'a, DataContext<V>>,
        edge_resolver: IndividualEdgeResolver<'a>,
    ) -> Self {
        Self { origin, contexts, edge_resolver }
    }
}

impl<'a, V: AsVertex<FilesystemVertex>> Iterator for EdgeResolverIterator<'a, V> {
    type Item = (DataContext<V>, VertexIterator<'a, FilesystemVertex>);

    fn next(&mut self) -> Option<ContextAndIterableOfEdges<'a, V>> {
        if let Some(context) = self.contexts.next() {
            if let Some(vertex) = context.active_vertex::<FilesystemVertex>() {
                let neighbors = (self.edge_resolver)(self.origin.clone(), vertex);
                Some((context, neighbors))
            } else {
                let empty_iterator: iter::Empty<FilesystemVertex> = iter::empty();
                Some((context, Box::new(empty_iterator)))
            }
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilesystemVertex {
    Directory(DirectoryVertex),
    File(FileVertex),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryVertex {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileVertex {
    pub name: String,
    pub extension: Option<String>,
    pub path: String,
}

fn directory_contains_file_handler<'a>(
    origin: Rc<PathBuf>,
    vertex: &FilesystemVertex,
) -> VertexIterator<'a, FilesystemVertex> {
    let directory_vertex = match vertex {
        FilesystemVertex::Directory(dir) => dir,
        _ => unreachable!(),
    };
    Box::from(DirectoryContainsFileIterator::new(origin, directory_vertex))
}

fn directory_subdirectory_handler<'a>(
    origin: Rc<PathBuf>,
    vertex: &FilesystemVertex,
) -> VertexIterator<'a, FilesystemVertex> {
    let directory_vertex = match vertex {
        FilesystemVertex::Directory(dir) => dir,
        _ => unreachable!(),
    };
    Box::from(SubdirectoryIterator::new(origin, directory_vertex))
}

#[allow(unused_variables)]
impl<'a> Adapter<'a> for FilesystemInterpreter {
    type Vertex = FilesystemVertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexIterator<'a, Self::Vertex> {
        assert!(edge_name.as_ref() == "OriginDirectory");
        assert!(parameters.is_empty());
        let vertex = DirectoryVertex { name: "<origin>".to_owned(), path: "".to_owned() };
        Box::new(OriginIterator::new(vertex))
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, FieldValue> {
        match type_name.as_ref() {
            "Directory" => {
                match property_name.as_ref() {
                    "name" => Box::new(contexts.map(|context| {
                        match context.active_vertex::<Self::Vertex>() {
                            None => (context, FieldValue::Null),
                            Some(FilesystemVertex::Directory(x)) => {
                                let value = FieldValue::String(x.name.clone().into());
                                (context, value)
                            }
                            _ => unreachable!(),
                        }
                    })),
                    "path" => Box::new(contexts.map(|context| {
                        match context.active_vertex::<Self::Vertex>() {
                            None => (context, FieldValue::Null),
                            Some(FilesystemVertex::Directory(x)) => {
                                let value = FieldValue::String(x.path.clone().into());
                                (context, value)
                            }
                            _ => unreachable!(),
                        }
                    })),
                    "__typename" => Box::new(contexts.map(|context| {
                        match context.active_vertex::<Self::Vertex>() {
                            None => (context, FieldValue::Null),
                            Some(_) => (context, "Directory".into()),
                        }
                    })),
                    _ => todo!(),
                }
            }
            "File" => {
                match property_name.as_ref() {
                    "name" => Box::new(contexts.map(|context| {
                        match context.active_vertex::<Self::Vertex>() {
                            None => (context, FieldValue::Null),
                            Some(FilesystemVertex::File(x)) => {
                                let value = FieldValue::String(x.name.clone().into());
                                (context, value)
                            }
                            _ => unreachable!(),
                        }
                    })),
                    "path" => Box::new(contexts.map(|context| {
                        match context.active_vertex::<Self::Vertex>() {
                            None => (context, FieldValue::Null),
                            Some(FilesystemVertex::File(x)) => {
                                let value = FieldValue::String(x.path.clone().into());
                                (context, value)
                            }
                            _ => unreachable!(),
                        }
                    })),
                    "extension" => Box::new(contexts.map(|context| {
                        match context.active_vertex::<Self::Vertex>() {
                            None => (context, FieldValue::Null),
                            Some(FilesystemVertex::File(x)) => {
                                let value =
                                    x.extension.clone().map(Into::into).unwrap_or(FieldValue::Null);
                                (context, value)
                            }
                            _ => unreachable!(),
                        }
                    })),
                    "__typename" => Box::new(contexts.map(|context| {
                        match context.active_vertex::<Self::Vertex>() {
                            None => (context, FieldValue::Null),
                            Some(_) => (context, "File".into()),
                        }
                    })),
                    _ => todo!(),
                }
            }
            _ => todo!(),
        }
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Self::Vertex>> {
        match (type_name.as_ref(), edge_name.as_ref()) {
            ("Directory", "out_Directory_ContainsFile") => {
                let iterator = EdgeResolverIterator::new(
                    self.origin.clone(),
                    contexts,
                    directory_contains_file_handler,
                );
                Box::from(iterator)
            }
            ("Directory", "out_Directory_Subdirectory") => {
                let iterator = EdgeResolverIterator::new(
                    self.origin.clone(),
                    contexts,
                    directory_subdirectory_handler,
                );
                Box::from(iterator)
            }
            _ => unimplemented!(),
        }
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'a, V, bool> {
        todo!()
    }
}

fn join_with_slash(base: &str, name: &str) -> String {
    if base.is_empty() { name.to_owned() } else { format!("{base}/{name}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(name);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn collect_files(origin: &Path, dir_path: &str) -> Vec<FileVertex> {
        let directory = DirectoryVertex { name: "root".to_owned(), path: dir_path.to_owned() };
        let mut files: Vec<FileVertex> =
            DirectoryContainsFileIterator::new(Rc::new(origin.to_path_buf()), &directory)
                .map(|v| match v {
                    FilesystemVertex::File(f) => f,
                    _ => panic!("expected file vertex"),
                })
                .collect();
        files.sort_by(|a, b| a.name.cmp(&b.name));
        files
    }

    fn collect_dirs(origin: &Path, dir_path: &str) -> Vec<DirectoryVertex> {
        let directory = DirectoryVertex { name: "root".to_owned(), path: dir_path.to_owned() };
        let mut dirs: Vec<DirectoryVertex> =
            SubdirectoryIterator::new(Rc::new(origin.to_path_buf()), &directory)
                .map(|v| match v {
                    FilesystemVertex::Directory(d) => d,
                    _ => panic!("expected directory vertex"),
                })
                .collect();
        dirs.sort_by(|a, b| a.name.cmp(&b.name));
        dirs
    }

    #[test]
    fn file_iterator_yields_files_with_correct_fields() {
        let dir = TempDir::new("trustfall_test_file_iter");
        fs::write(dir.path().join("foo.txt"), "").unwrap();
        fs::write(dir.path().join("bar.rs"), "").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();

        let files = collect_files(dir.path(), "");

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name, "bar.rs");
        assert_eq!(files[0].extension, Some("rs".to_owned()));
        assert_eq!(files[0].path, "bar.rs");
        assert_eq!(files[1].name, "foo.txt");
        assert_eq!(files[1].extension, Some("txt".to_owned()));
        assert_eq!(files[1].path, "foo.txt");
    }

    #[test]
    fn file_iterator_paths_use_forward_slashes() {
        let dir = TempDir::new("trustfall_test_file_slash");
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub").join("deep.txt"), "").unwrap();

        let files = collect_files(dir.path(), "sub");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "sub/deep.txt");
        assert!(!files[0].path.contains('\\'));
    }

    #[test]
    fn file_iterator_handles_no_extension() {
        let dir = TempDir::new("trustfall_test_no_ext");
        fs::write(dir.path().join("Makefile"), "").unwrap();

        let files = collect_files(dir.path(), "");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "Makefile");
        assert_eq!(files[0].extension, None);
    }

    #[test]
    fn file_iterator_empty_on_missing_directory() {
        let origin = std::env::temp_dir().join("trustfall_test_nonexistent_xyz");
        let files = collect_files(&origin, "");
        assert!(files.is_empty());
    }

    #[test]
    fn subdir_iterator_yields_directories_with_correct_fields() {
        let dir = TempDir::new("trustfall_test_subdir_iter");
        fs::create_dir(dir.path().join("alpha")).unwrap();
        fs::create_dir(dir.path().join("beta")).unwrap();
        fs::write(dir.path().join("file.txt"), "").unwrap();

        let dirs = collect_dirs(dir.path(), "");

        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[0].name, "alpha");
        assert_eq!(dirs[0].path, "alpha");
        assert_eq!(dirs[1].name, "beta");
        assert_eq!(dirs[1].path, "beta");
    }

    #[test]
    fn subdir_iterator_paths_use_forward_slashes() {
        let dir = TempDir::new("trustfall_test_subdir_slash");
        fs::create_dir(dir.path().join("parent")).unwrap();
        fs::create_dir(dir.path().join("parent").join("child")).unwrap();

        let dirs = collect_dirs(dir.path(), "parent");

        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].path, "parent/child");
        assert!(!dirs[0].path.contains('\\'));
    }

    #[test]
    fn subdir_iterator_skips_hidden_and_build_dirs() {
        let dir = TempDir::new("trustfall_test_skip_dirs");
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::create_dir(dir.path().join(".vscode")).unwrap();
        fs::create_dir(dir.path().join("target")).unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();

        let dirs = collect_dirs(dir.path(), "");

        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].name, "src");
    }

    #[test]
    fn subdir_iterator_empty_on_missing_directory() {
        let origin = std::env::temp_dir().join("trustfall_test_nonexistent_xyz");
        let dirs = collect_dirs(&origin, "");
        assert!(dirs.is_empty());
    }
}
