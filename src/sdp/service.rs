use std::fmt::Debug;
use std::ops::RangeInclusive;
use std::sync::Arc;
use crate::sdp::data_element::{DataElement, Uuid};

#[derive(Clone, Eq, PartialEq)]
pub struct ServiceAttribute {
    pub id: u16,
    pub value: DataElement
}

impl ServiceAttribute {
    pub fn new<T: Into<DataElement>>(id: u16, value: T) -> Self {
        Self {id, value: value.into() }
    }

    pub fn contains(&self, uuid: Uuid) -> bool {
        fn contains(v: &DataElement, uuid: Uuid) -> bool {
            match v {
                DataElement::Uuid(value) => *value == uuid,
                DataElement::Sequence(values) => values.iter().any(|v| contains(v, uuid)),
                _ => false
            }
        }
        contains(&self.value, uuid)
    }

}

impl IntoIterator for ServiceAttribute {
    type Item = DataElement;
    type IntoIter = std::array::IntoIter<Self::Item, 2>;

    fn into_iter(self) -> Self::IntoIter {
        [self.id.into(), self.value].into_iter()
    }

}

impl Debug for ServiceAttribute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("")
            .field(&self.id)
            .field(&self.value)
            .finish()
    }
}

//#[derive(Clone)]
pub struct Service {
    attributes: Arc<Vec<ServiceAttribute>>
}

impl Service {

    pub fn contains(&self, uuid: Uuid) -> bool {
        self.attributes.iter().any(|a| a.contains(uuid))
    }

    pub fn attributes<'a: 'b, 'b>(&'a self, requested: &'b [RangeInclusive<u16>]) -> impl Iterator<Item=&'a ServiceAttribute> + 'b {
        self.attributes
            .iter()
            .filter(move |a| requested
                .iter()
                .any(|r| r.contains(&a.id)))
    }

}

impl AsRef<[ServiceAttribute]> for Service {
    fn as_ref(&self) -> &[ServiceAttribute] {
        &self.attributes
    }
}

impl FromIterator<ServiceAttribute> for Service {
    fn from_iter<T: IntoIterator<Item=ServiceAttribute>>(iter: T) -> Self {
        let mut attributes = iter.into_iter().collect::<Vec<_>>();
        attributes.sort_by_key(|a| a.id);
        Self {
            attributes: Arc::new(attributes)
        }
    }
}

impl Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.attributes.iter()).finish()
    }
}