use super::{Collection, CollectionId, ObjectId, ObjectInstance, Slot, Value};
use crate::hir::ClassMemberId;
use std::collections::HashMap;

/// Mutable language data owned by one isolated execution.
///
/// The checked runtime image is borrowed and shareable. Collection/object
/// identity and static member state live here so a fresh store remains the
/// unit of execution and test isolation.
#[derive(Default)]
pub(super) struct ExecutionStore {
    collections: Vec<Collection>,
    objects: Vec<ObjectInstance>,
    static_fields: HashMap<ClassMemberId, Slot>,
}

impl ExecutionStore {
    pub(super) fn allocate_collection(&mut self, collection: Collection) -> Value {
        let id = CollectionId(self.collections.len());
        self.collections.push(collection);
        Value::Collection(id)
    }

    pub(super) fn collection(&self, id: CollectionId) -> &Collection {
        self.collections
            .get(id.0)
            .expect("runtime collection handles are always valid")
    }

    pub(super) fn collection_mut(&mut self, id: CollectionId) -> &mut Collection {
        self.collections
            .get_mut(id.0)
            .expect("runtime collection handles are always valid")
    }

    pub(super) fn allocate_object(&mut self, class_id: usize) -> ObjectId {
        let id = ObjectId(self.objects.len());
        self.objects.push(ObjectInstance {
            class_id,
            fields: HashMap::new(),
        });
        id
    }

    pub(super) fn object(&self, id: ObjectId) -> &ObjectInstance {
        self.objects
            .get(id.0)
            .expect("runtime object handles are always valid")
    }

    pub(super) fn object_mut(&mut self, id: ObjectId) -> &mut ObjectInstance {
        self.objects
            .get_mut(id.0)
            .expect("runtime object handles are always valid")
    }

    pub(super) fn insert_static_slot(&mut self, target: ClassMemberId, slot: Slot) {
        self.static_fields.insert(target, slot);
    }

    pub(super) fn static_slot(&self, target: &ClassMemberId) -> Option<&Slot> {
        self.static_fields.get(target)
    }

    pub(super) fn static_slot_mut(&mut self, target: &ClassMemberId) -> Option<&mut Slot> {
        self.static_fields.get_mut(target)
    }
}
