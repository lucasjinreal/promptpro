use crate::{PromptVault, VersionMeta, VersionSelector};
use pyo3::prelude::*;

/// Python wrapper for VersionMeta
#[pyclass]
#[derive(Clone)]
pub struct PyVersionMeta {
    #[pyo3(get)]
    pub key: String,
    #[pyo3(get)]
    pub version: u64,
    #[pyo3(get)]
    pub timestamp: String, // Convert DateTime to string for Python
    #[pyo3(get)]
    pub parent: Option<u64>,
    #[pyo3(get)]
    pub message: Option<String>,
    #[pyo3(get)]
    pub object_hash: String,
    #[pyo3(get)]
    pub snapshot: bool,
    #[pyo3(get)]
    pub tags: Vec<String>,
}

impl From<VersionMeta> for PyVersionMeta {
    fn from(meta: VersionMeta) -> Self {
        PyVersionMeta {
            key: meta.key,
            version: meta.version,
            timestamp: meta.timestamp.to_rfc3339(),
            parent: meta.parent,
            message: meta.message,
            object_hash: meta.object_hash,
            snapshot: meta.snapshot,
            tags: meta.tags,
        }
    }
}

/// Python wrapper for PromptVault
#[pyclass]
pub struct PyPromptVault {
    inner: PromptVault,
}

#[pymethods]
impl PyPromptVault {
    /// Create a new PromptVault at the specified path
    #[new]
    fn new(path: Option<String>) -> PyResult<Self> {
        let vault = match path {
            Some(p) => PromptVault::open(std::path::Path::new(&p))
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?,
            None => PromptVault::open_default()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?,
        };

        Ok(PyPromptVault { inner: vault })
    }

    /// Add a new prompt with the given key and content
    fn add(&self, key: &str, content: &str) -> PyResult<()> {
        self.inner
            .add(key, content)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }

    /// Update an existing prompt with new content
    fn update(&self, key: &str, content: &str, message: Option<String>) -> PyResult<()> {
        self.inner
            .update(key, content, message)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }

    /// Get prompt content by key and selector
    fn get(&self, key: &str, selector: &PyAny) -> PyResult<String> {
        let version_selector = parse_version_selector(selector)?;
        self.inner
            .get(key, version_selector)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }

    /// Get the latest version of a prompt
    fn get_latest(&self, key: &str) -> PyResult<String> {
        self.inner
            .get(key, VersionSelector::Latest)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }

    /// Get history of all versions for a key
    fn history(&self, key: &str) -> PyResult<Vec<PyVersionMeta>> {
        let versions = self
            .inner
            .history(key)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?;

        Ok(versions.into_iter().map(PyVersionMeta::from).collect())
    }

    /// Tag a specific version
    fn tag(&self, key: &str, tag: &str, version: u64) -> PyResult<()> {
        self.inner
            .tag(key, tag, version)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }

    /// Promote a tag to point to the latest version
    fn promote(&self, key: &str, tag: &str) -> PyResult<()> {
        self.inner
            .promote(key, tag)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }

    /// Dump the vault to a binary file
    fn dump(&self, output_path: &str, password: Option<&str>) -> PyResult<()> {
        self.inner
            .dump(output_path, password)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }

    /// Restore a vault from a binary file
    #[staticmethod]
    fn restore(input_path: &str, password: Option<&str>) -> PyResult<PyPromptVault> {
        let vault = PromptVault::restore(input_path, password)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?;

        Ok(PyPromptVault { inner: vault })
    }

    #[staticmethod]
    fn restore_or_default(input_path: &str, password: Option<&str>) -> PyResult<PyPromptVault> {
        let vault = PromptVault::restore_or_default(input_path, password)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?;
        Ok(PyPromptVault { inner: vault })
    }

    /// Get the latest version number for a key
    fn get_latest_version_number(&self, key: &str) -> PyResult<Option<u64>> {
        self.inner
            .get_latest_version_number(key)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }
}

/// Parse Python object to VersionSelector
fn parse_version_selector(selector: &PyAny) -> PyResult<VersionSelector> {
    use pyo3::types::PyString;

    if selector.is_none() {
        Ok(VersionSelector::Latest)
    } else if let Ok(version) = selector.extract::<u64>() {
        Ok(VersionSelector::Version(version))
    } else if let Ok(tag) = selector.extract::<String>() {
        if tag == "latest" {
            Ok(VersionSelector::Latest)
        } else {
            Ok(VersionSelector::Tag(Box::leak(tag.into_boxed_str())))
        }
    } else if let Ok(tag) = selector.downcast::<PyString>() {
        let tag_str = tag.to_str()?;
        if tag_str == "latest" {
            Ok(VersionSelector::Latest)
        } else {
            Ok(VersionSelector::Tag(Box::leak(
                tag_str.to_string().into_boxed_str(),
            )))
        }
    } else {
        Err(pyo3::exceptions::PyValueError::new_err(
            "Invalid version selector. Must be a string (tag) or integer (version).",
        ))
    }
}

/// Python wrapper for SyncPromptManager
#[pyclass]
pub struct PySyncPromptManager {
    inner: crate::sync_api::SyncPromptManager,
}

#[pymethods]
impl PySyncPromptManager {
    #[new]
    fn new(path: Option<String>) -> PyResult<Self> {
        let manager = match path {
            Some(p) => crate::sync_api::SyncPromptManager::with_path(std::path::Path::new(&p))
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?,
            None => crate::sync_api::SyncPromptManager::new()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?,
        };

        Ok(PySyncPromptManager { inner: manager })
    }

    /// Get the singleton instance
    #[staticmethod]
    fn get() -> PyResult<PySyncPromptManager> {
        Ok(PySyncPromptManager {
            inner: crate::sync_api::SyncPromptManager::get().clone(),
        })
    }

    /// Add a prompt
    fn add(&self, key: &str, content: &str) -> PyResult<()> {
        self.inner
            .add(key, content)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }

    /// Update a prompt
    fn update(&self, key: &str, content: &str, message: Option<&str>) -> PyResult<()> {
        self.inner
            .update(key, content, message)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }

    /// Tag a version
    fn tag(&self, key: &str, tag: &str, version: u64) -> PyResult<()> {
        self.inner
            .tag(key, tag, version)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }

    /// Get a prompt by selector
    fn get_prompt(&self, key: &str, selector: &PyAny) -> PyResult<String> {
        let version_selector = parse_version_selector(selector)?;
        self.inner
            .get_prompt(key, version_selector)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }

    /// Get latest version of a prompt
    fn latest(&self, key: &str) -> PyResult<String> {
        self.inner
            .latest(key)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }

    /// Get history of a prompt
    fn history(&self, key: &str) -> PyResult<Vec<PyVersionMeta>> {
        let versions = self
            .inner
            .history(key)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?;

        Ok(versions.into_iter().map(PyVersionMeta::from).collect())
    }

    /// Backup the vault
    fn backup(&self, path: &str, password: Option<&str>) -> PyResult<()> {
        self.inner
            .backup(path, password)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))
    }
}

/// Python module initialization
#[pymodule]
fn promptpro(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyPromptVault>()?;
    m.add_class::<PyVersionMeta>()?;
    m.add_class::<PySyncPromptManager>()?;
    Ok(())
}
