use std::ops::{Deref, DerefMut};

use aws_smithy_runtime_api::client::orchestrator::Metadata;
use aws_smithy_types::config_bag::{ConfigBag, Storable, StoreReplace};

#[derive(Debug)]
pub struct StorableOption<T: core::fmt::Debug>(Option<T>);

impl<T: core::fmt::Debug + Send + Sync + 'static> Storable for StorableOption<T> {
    type Storer = StoreReplace<Self>;
}
impl<T: core::fmt::Debug> Default for StorableOption<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T: core::fmt::Debug> StorableOption<T> {
    pub fn new(content: T) -> Self {
        Self(Some(content))
    }
}

impl<T: core::fmt::Debug> Deref for StorableOption<T> {
    type Target = Option<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T: core::fmt::Debug> DerefMut for StorableOption<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub fn extract_service_operation(cfg: &ConfigBag) -> (super::Service<'_>, super::Operation<'_>) {
    let metadata = cfg.load::<Metadata>().expect("metadata always present");
    (metadata.service(), metadata.name())
}
