//! Provides an API to add and remove plans associated with a specific entity. This is especially
//! useful for managing plans for people that you want to cancel when the person dies.
//!
//! Keep in mind that every plan you want to be managed through this API *must* be added through
//! this API. Thus, if you have a recurring plan that schedules its next run when it fires, it
//! needs to schedule that next run through this API.
//!
//! To cancel a person's plans when they die, you will want to listen for the
//! `PropertyChangeEvent<Person, Alive>`:
//!
//! ```rust
//! context.subscribe_to_event::<PropertyChangeEvent<Person, Alive>>(
//!     |context, event| context.cancel_plans_for_entity(event.entity_id),
//! );
//! ```

use ixa::entity::entity_store::get_registered_entity_count;
use ixa::plan::PlanId;
use ixa::prelude::*;
use ixa::{ContextBase, ExecutionPhase, HashMap};
use std::any::Any;
use std::cell::OnceCell;

define_data_plugin!(PlanIndexPlugin, EntityPlanIndexStore, |_context| {
    EntityPlanIndexStore::new()
});

/// A lightweight data plugin container to store the entity plan indexes.
/// This is essentially an implementation of `EntityStore` in "userland".
struct EntityPlanIndexStore {
    /// The indexes each stored at their corresponding `E::id()`.
    indexes: Vec<OnceCell<Box<dyn Any>>>,
}

impl EntityPlanIndexStore {
    /// Creates a new [`EntityPlanIndexStore`], allocating the exact number of slots as there are
    /// registered [`Entity`]s.
    pub fn new() -> Self {
        let num_items = get_registered_entity_count();
        Self {
            indexes: (0..num_items).map(|_| OnceCell::new()).collect(),
        }
    }

    fn get_plan_index<E: Entity>(&self) -> &EntityPlanIndex<E> {
        let index = E::id();
        let record = self.indexes.get(index).unwrap_or_else(|| {
            panic!(
                "No registered entity found with index = {index:?}. You must use the `define_entity!` macro to create an entity."
            )
        });
        let plan_index = record.get_or_init(|| {
            Box::new(EntityPlanIndex::<E>::default()) as Box<dyn Any>
        });
        plan_index.downcast_ref::<EntityPlanIndex<E>>().expect(
            "TypeID does not match registered item type. You must use the `define_registered_item!` macro to create a registered item.",
        )
    }

    fn get_plan_index_mut<E: Entity>(&mut self) -> &mut EntityPlanIndex<E> {
        let index = E::id();
        let record = self.indexes.get_mut(index).unwrap_or_else(|| {
            panic!(
                "No registered entity found with index = {index:?}. You must use the `define_entity!` macro to create an entity."
            )
        });
        let _ = record.get_or_init(|| {
            Box::new(EntityPlanIndex::<E>::default()) as Box<dyn Any>
        });
        let plan_index = record.get_mut().unwrap();
        plan_index.as_mut().downcast_mut::<EntityPlanIndex<E>>().expect(
            "TypeID does not match registered item type. You must use the `define_registered_item!` macro to create a registered item.",
        )
    }
}

type EntityPlanIndex<E> = HashMap<EntityId<E>, Vec<PlanId>>;

pub trait PlanIndexContextExt: ContextBase {
    /// Adds a plan for the given entity, recording in the index the fact that this plan ID is
    /// associated with this entity.
    fn add_plan_for_entity<E: Entity>(
        &mut self,
        entity_id: EntityId<E>,
        time: f64,
        callback: impl FnOnce(&mut Context) + 'static
    ) -> PlanId {
        self.add_plan_for_entity_with_phase(entity_id, time, callback, ExecutionPhase::Normal)
    }

    /// Adds a plan for the given entity to be executed at the given execution phase, recording in
    /// the index the fact that this plan ID is associated with this entity.
    fn add_plan_for_entity_with_phase<E: Entity>(
        &mut self,
        entity_id: EntityId<E>,
        time: f64,
        callback: impl FnOnce(&mut Context) + 'static,
        phase: ExecutionPhase,
    ) -> PlanId {
        // Here is the fundamental problem with trying to implement this in client code.
        // We need to know the plan ID of the plan that is currently being handled in order to
        // maintain the plan index, but we don't know the plan ID until _after_ we add the plan.
        // Our solution is to just keep growing the index. No harm comes from trying to cancel
        // a plan that has already been executed.
        let new_plan_id = self.add_plan_with_phase(
            time,
            callback,
            // |context| {
            //     // Remove the plan ID from the index when the plan fires.
            //     let plan_index_data = self.get_data_mut(PlanIndexPlugin);
            //     let plan_index = plan_index_data.get_plan_index_mut::<E>();
            //     plan_index.entry(entity_id).and_modify(|v| v.retain(|id| id != new_plan_id));
            //     callback(context)
            // }
            phase
        );
        let plan_index_data = self.get_data_mut(PlanIndexPlugin);
        let plan_index = plan_index_data.get_plan_index_mut::<E>();
        plan_index.entry(entity_id).or_default().push(new_plan_id);
        new_plan_id
    }

    /// Fetches a copy of the vector of plan IDs scheduled for this entity.
    fn get_plans_for_entity<E: Entity>(&self, entity_id: EntityId<E>) -> Vec<PlanId> {
        let plan_index_data = self.get_data(PlanIndexPlugin);
        let plan_index = plan_index_data.get_plan_index::<E>();
        plan_index.get(&entity_id).cloned().unwrap_or_default()
    }

    fn cancel_plans_for_entity<E: Entity>(&mut self, entity_id: EntityId<E>) -> bool {
        let plan_index_data = self.get_data_mut(PlanIndexPlugin);
        let plan_index = plan_index_data.get_plan_index_mut::<E>();

        if let Some(plans) = plan_index.remove(&entity_id) {
            // Replace with `Context::cancel_plans_unchecked` when it lands so we avoid
            for plan_id in plans {
                self.cancel_plan(&plan_id);
            }
            true
        } else {
            false
        }
    }
}
