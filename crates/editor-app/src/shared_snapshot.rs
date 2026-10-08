//! UI-owned immutable snapshots. Writes detach; cached identities cannot alias a write.
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq)]
pub struct SnapshotVec<T: Clone>(Arc<Vec<T>>);

impl<T: Clone> Default for SnapshotVec<T> {
    fn default() -> Self {
        Self(Arc::new(Vec::new()))
    }
}
impl<T: Clone> From<Vec<T>> for SnapshotVec<T> {
    fn from(items: Vec<T>) -> Self {
        Self(Arc::new(items))
    }
}
impl<T: Clone> FromIterator<T> for SnapshotVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(items: I) -> Self {
        Vec::from_iter(items).into()
    }
}
impl<T: Clone> Deref for SnapshotVec<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T: Clone> DerefMut for SnapshotVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}
impl<T: Clone> IntoIterator for SnapshotVec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        Arc::unwrap_or_clone(self.0).into_iter()
    }
}
impl<'a, T: Clone> IntoIterator for &'a SnapshotVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, T: Clone> IntoIterator for &'a mut SnapshotVec<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
impl<T: Clone + PartialEq> PartialEq<Vec<T>> for SnapshotVec<T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.0.as_ref() == other
    }
}
impl<T: Clone + serde::Serialize> serde::Serialize for SnapshotVec<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_ref().serialize(serializer)
    }
}
impl<T: Clone> SnapshotVec<T> {
    pub fn clear(&mut self) {
        if let Some(items) = Arc::get_mut(&mut self.0) {
            items.clear();
        } else {
            // Clearing must not clone a large shared selection merely to discard it.
            self.0 = Arc::new(Vec::new());
        }
    }
    pub fn shares_storage(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clones_and_writes_preserve_old_snapshots_and_json_array_shape() {
        let original: SnapshotVec<u32> = vec![1, 2, 3].into();
        let mut changed = original.clone();
        assert!(changed.shares_storage(&original));
        changed[1] = 9;
        assert!(!changed.shares_storage(&original));
        assert_eq!(original.as_slice(), &[1, 2, 3]);
        changed.push(4);
        assert_eq!(
            serde_json::to_value(&changed).unwrap(),
            serde_json::json!([1, 9, 3, 4])
        );
        let mut second = changed.clone();
        for n in &mut second {
            *n += 1;
        }
        assert_eq!(changed.into_iter().collect::<Vec<_>>(), vec![1, 9, 3, 4]);
        assert_eq!(second.into_iter().collect::<Vec<_>>(), vec![2, 10, 4, 5]);
    }
    #[test]
    fn floating_equality_remains_vec_equality_even_for_shared_storage() {
        let nan: SnapshotVec<f64> = vec![f64::NAN].into();
        assert_ne!(nan, nan.clone());
        let zero: SnapshotVec<f64> = vec![-0.0].into();
        assert_eq!(zero, SnapshotVec::from(vec![0.0]));
        assert_eq!(
            serde_json::to_value(vec![1u32, 2]).unwrap(),
            serde_json::to_value(SnapshotVec::from(vec![1u32, 2])).unwrap()
        );
    }
}
