use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::num::{NonZeroU64, TryFromIntError};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use via::Error;
use via::error::BoxError;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Id(NonZeroU64);

pub trait Identify {
    fn id(&self) -> &Id;
}

pub trait Persist {
    type Output;
    type Error: Send + Sync;

    fn persist(self, id: Id) -> Result<Self::Output, Self::Error>;
}

#[derive(Deserialize, Serialize)]
pub struct Table<T> {
    store: BTreeMap<Id, T>,
    offset: AtomicU64,
}

impl<T> Table<T>
where
    T: Clone + Identify,
{
    pub fn new() -> Self {
        Self {
            store: BTreeMap::new(),
            offset: AtomicU64::new(1),
        }
    }

    pub fn get(&self, id: &Id) -> Option<T> {
        self.store.get(id).cloned()
    }

    pub fn insert<U>(&mut self, row: U) -> Result<T, BoxError>
    where
        BoxError: From<U::Error>,
        U: Persist<Output = T>,
    {
        let id = Id::new(self.offset.fetch_add(1, Ordering::Relaxed))?;

        self.store.insert(id, row.persist(id)?);
        Ok(self.store.get(&id).cloned().ok_or("insert failed.")?)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.store.values()
    }

    pub fn remove(&mut self, id: &Id) {
        self.store.remove(id);
    }
}

impl Id {
    #[inline]
    pub fn new(value: u64) -> Result<Self, TryFromIntError> {
        value.try_into().map(Self)
    }

    pub fn to_bytes(self) -> [u8; 8] {
        self.0.get().to_be_bytes()
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromStr for Id {
    type Err = BoxError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Ok(Self(input.parse()?))
    }
}
