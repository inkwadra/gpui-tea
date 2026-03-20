use crate::ChildPath;
use std::{fmt, sync::Arc};

#[derive(Clone, PartialEq, Eq, Hash)]
/// Identify a keyed effect or subscription across reconciliations.
///
/// A `Key` is stable application-level identity. The runtime augments keys created in child
/// models with their [`ChildPath`] so sibling models can reuse the same local identifiers without
/// colliding.
pub struct Key {
    id: Arc<str>,
    child_path: Option<ChildPath>,
    local_id: Option<Arc<str>>,
}

impl Key {
    /// Build a key from an application-defined identifier.
    pub fn new(id: impl Into<Arc<str>>) -> Self {
        Self {
            id: id.into(),
            child_path: None,
            local_id: None,
        }
    }

    /// Return the runtime identifier used for equality and deduplication.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Return the child path that scoped this key, if any.
    #[must_use]
    pub fn child_path(&self) -> Option<&ChildPath> {
        self.child_path.as_ref()
    }

    /// Return the original caller-provided identifier for a scoped key.
    #[must_use]
    pub fn local_id(&self) -> Option<&str> {
        self.local_id.as_deref()
    }

    /// Return a new key scoped to the provided path.
    pub(crate) fn scoped(&self, path: &ChildPath) -> Self {
        if path.is_root() || self.child_path.is_some() {
            return self.clone();
        }

        let local_id: Arc<str> = Arc::from(self.id());
        let mut runtime_id = String::from(path.runtime_prefix().as_ref());
        runtime_id.push('#');
        runtime_id.push_str(&local_id.len().to_string());
        runtime_id.push(':');
        runtime_id.push_str(local_id.as_ref());

        Self {
            id: Arc::from(runtime_id),
            child_path: Some(path.clone()),
            local_id: Some(local_id),
        }
    }
}

impl<T: Into<Arc<str>>> From<T> for Key {
    fn from(id: T) -> Self {
        Self::new(id)
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let (Some(path), Some(local_id)) = (&self.child_path, &self.local_id) {
            return write!(formatter, "Key(\"{path}:{local_id}\")");
        }

        write!(formatter, "Key(\"{}\")", self.id)
    }
}
