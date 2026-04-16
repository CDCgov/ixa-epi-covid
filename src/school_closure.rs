use ixa::prelude::*;

use crate::{ContextParametersExt, population_loader::{ItineraryRatios, Person, SchoolId, WorkId}, settings::{ContextSettingExt, SETTING_COUNT, Setting, SettingCategory, SettingId}};

static SCHOOL_CLOSURE_RATIOS: [f64;SETTING_COUNT] = [0.75, 0.0, 0.0, 0.25];
static SCHOOL_CLOSURE_WITH_WORK_RATIOS: [f64;SETTING_COUNT] = [0.5, 0.25, 0.0, 0.25];
static SCHOOL_OPEN_RATIOS: [f64;SETTING_COUNT] = [1.0/3.0, 0.0, 1.0/3.0, 1.0/3.0];
static SCHOOL_OPEN_WITH_WORK_RATIOS: [f64;SETTING_COUNT] = [0.25, 0.25, 0.25, 0.25];


pub trait ContextSchoolClosureExt: PluginContext + ContextEntitiesExt + ContextParametersExt + ContextSettingExt {
    fn close_school(&mut self, setting: SettingId){
        let people: Vec<_> = self.query_result_iterator::<Person, _>((SchoolId(Some(setting)),)).collect();
        for person in people {
            if self.get_property::<Person, WorkId>(person).0.is_some() {
                self.set_property::<Person, ItineraryRatios>(
                    person,
                    ItineraryRatios {
                        itinerary_ratios: SCHOOL_CLOSURE_WITH_WORK_RATIOS,
                    },
                );
            } else {
                self.set_property::<Person, ItineraryRatios>(
                    person,
                    ItineraryRatios {
                        itinerary_ratios: SCHOOL_CLOSURE_RATIOS,
                    },
                );
            }
            let _ = self.decrement_setting_size(setting);
        }
        println!("Setting size after closing school with SettingId {:?}: {:?}", setting, self.get_setting_size(setting).unwrap());
    }

    fn open_school(&mut self, setting: SettingId){
        let people: Vec<_> = self.query_result_iterator::<Person, _>((SchoolId(Some(setting)),)).collect();
        for person in people {
            if self.get_property::<Person, WorkId>(person).0.is_some() {
                self.set_property::<Person, ItineraryRatios>(
                    person,
                    ItineraryRatios {
                        itinerary_ratios: SCHOOL_OPEN_WITH_WORK_RATIOS,
                    },
                );
            } else {
                self.set_property::<Person, ItineraryRatios>(
                    person,
                    ItineraryRatios {
                        itinerary_ratios: SCHOOL_OPEN_RATIOS,
                    },
                );

            }
            let _ = self.increment_setting_size(setting);
        }
        println!("Setting size after opening school with SettingId {:?}: {:?}", setting, self.get_setting_size(setting).unwrap());
    }

    fn add_school_closure_and_reopening(&mut self) {
        self.add_plan(1.0, move |context| {
            for setting in context.get_entity_iterator::<Setting>() {
                let setting_category = context.get_property::<Setting, SettingCategory>(setting);
                if setting_category == SettingCategory::School {
                    println!("Closing school with SettingId {:?}", setting);
                    context.close_school(setting);
                }
            }
        });

        self.add_plan(40.0, move |context| {
            for setting in context.get_entity_iterator::<Setting>() {
                let setting_category = context.get_property::<Setting, SettingCategory>(setting);
                if setting_category == SettingCategory::School {
                    context.open_school(setting);
                }
            }
        });
    }
}
impl ContextSchoolClosureExt for Context {}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    context.add_school_closure_and_reopening();
    Ok(())
}