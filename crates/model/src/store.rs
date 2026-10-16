use std::path::{
    Path,
    PathBuf,
};
use std::{
    fs,
    io,
};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::catalog::Catalog;
use crate::ir::Spec;

pub const DATA_ENV: &str = "UAREFERENCE_DATA";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("no data directory: set {DATA_ENV} or HOME")]
    NoDataDir,
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("{path}: {source}")]
    Json { path: PathBuf, source: serde_json::Error },
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// The on-disk layout: `catalog.json`, `raw/<id>.xml`, `specs/<id>.json`, `state.json`.
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn open_default() -> Result<Self> {
        if let Some(dir) = std::env::var_os(DATA_ENV) {
            return Ok(Self::at(dir));
        }
        directories::ProjectDirs::from("", "", "uareference")
            .map(|dirs| Self::at(dirs.data_dir()))
            .ok_or(StoreError::NoDataDir)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn catalog_path(&self) -> PathBuf {
        self.root.join("catalog.json")
    }

    pub fn state_path(&self) -> PathBuf {
        self.root.join("state.json")
    }

    pub fn spec_path(
        &self,
        id: &str,
    ) -> PathBuf {
        self.root.join("specs").join(format!("{id}.json"))
    }

    pub fn raw_path(
        &self,
        id: &str,
    ) -> PathBuf {
        self.root.join("raw").join(format!("{id}.xml"))
    }

    pub fn load_catalog(&self) -> Result<Catalog> {
        let path = self.catalog_path();
        if !path.exists() {
            return Ok(Catalog::default());
        }
        read_json(&path)
    }

    pub fn save_catalog(
        &self,
        catalog: &Catalog,
    ) -> Result<()> {
        write_json(&self.catalog_path(), catalog)
    }

    pub fn load_spec(
        &self,
        id: &str,
    ) -> Result<Spec> {
        read_json(&self.spec_path(id))
    }

    pub fn save_spec(
        &self,
        spec: &Spec,
    ) -> Result<()> {
        write_json(&self.spec_path(&spec.id), spec)
    }

    pub fn has_spec(
        &self,
        id: &str,
    ) -> bool {
        self.spec_path(id).exists()
    }

    pub fn load_raw(
        &self,
        id: &str,
    ) -> Result<Vec<u8>> {
        let path = self.raw_path(id);
        fs::read(&path).map_err(|source| StoreError::Io { path, source })
    }

    pub fn save_raw(
        &self,
        id: &str,
        bytes: &[u8],
    ) -> Result<()> {
        write_atomic(&self.raw_path(id), bytes)
    }

    /// Ids of every specification with a stored IR file, sorted.
    pub fn spec_ids(&self) -> Result<Vec<String>> {
        let dir = self.root.join("specs");
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(StoreError::Io { path: dir, source }),
        };
        let mut ids: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                e.file_name()
                    .to_str()?
                    .strip_suffix(".json")
                    .map(str::to_owned)
            })
            .collect();
        ids.sort();
        Ok(ids)
    }
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).map_err(|source| StoreError::Io {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| StoreError::Json {
        path: path.to_owned(),
        source,
    })
}

pub fn write_json<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|source| StoreError::Json {
        path: path.to_owned(),
        source,
    })?;
    write_atomic(path, &bytes)
}

fn write_atomic(
    path: &Path,
    bytes: &[u8],
) -> Result<()> {
    let io_err = |source| StoreError::Io {
        path: path.to_owned(),
        source,
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_err)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(io_err)?;
    fs::rename(&tmp, path).map_err(io_err)
}
