use std::{path::PathBuf, sync::Arc};

use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    geography::{ContextGeographyExt, Geography, GeographyType, Region, RegionId},
    itinerary_modifiers::{
        AcceptanceFunction, ItineraryTransitionMatrix, define_itinerary_modifier,
    },
    population_loader::SchoolId,
    settings::{ContextSettingExt, Itinerary, Person, SettingCategory},
};
use ixa::{
    ExecutionPhase, IxaEvent, csv, impl_derived_property,
    prelude::*,
    triggers::{ContextTriggersExt, TimeTrigger, TriggerCriterion},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct SchoolState(pub Option<u8>);
impl_derived_property!(SchoolState, Person, [Itinerary], [], |itinerary| {
    SchoolState(
        itinerary.setting_ids[SettingCategory::School]
            .map(|code| Some(code.0.state_code()))
            .unwrap_or(None),
    )
});

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct SchoolCounty(pub Option<(u8, u16)>);
impl_derived_property!(SchoolCounty, Person, [Itinerary], [], |itinerary| {
    SchoolCounty(
        itinerary.setting_ids[SettingCategory::School]
            .map(|code| Some((code.0.state_code(), code.0.county_code())))
            .unwrap_or(None),
    )
});

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct SchoolCensusTract(pub Option<(u8, u16, u32)>);
impl_derived_property!(SchoolCensusTract, Person, [Itinerary], [], |itinerary| {
    SchoolCensusTract(
        itinerary.setting_ids[SettingCategory::School]
            .map(|code| {
                Some((
                    code.0.state_code(),
                    code.0.county_code(),
                    code.0.census_tract_code(),
                ))
            })
            .unwrap_or(None),
    )
});

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct SchoolClosureRecord {
    pub geography: GeographyType,
    pub state: Option<u8>,
    pub county: Option<u16>,
    pub census_tract: Option<u32>,
    pub start_time: f64,
    pub end_time: f64,
}
impl SchoolClosureRecord {
    fn create_region_from_record(self, context: &mut Context) -> Result<RegionId, ModelError> {
        match (self.state, self.county, self.census_tract) {
            (Some(state), None, None) => Ok(context.create_region(Geography::State(state))),
            (Some(state), Some(county), None) => {
                Ok(context.create_region(Geography::County(state, county)))
            }
            (Some(state), Some(county), Some(tract)) => {
                Ok(context.create_region(Geography::CensusTract(state, county, tract)))
            }
            _ => Err(ModelError::ModelError(
                "Invalid geography configuration in SchoolClosureRecord".to_string(),
            )),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SchoolClosuresFromFile {
    pub include: bool,
    pub filename: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct Student(pub bool);

impl_derived_property!(Student, Person, [Itinerary], [], |itinerary| {
    Student(itinerary.setting_ids[SettingCategory::School].is_some())
});

define_multi_property!(Person, (SchoolId, Student));

#[derive(IxaEvent)]
struct SchoolClosure {
    active: bool,
    region: RegionId,
}

#[derive(Default)]
pub struct SchoolClosureData {
    dominant_school_closures: Vec<RegionId>,
    overridden_school_closures: Vec<RegionId>,
}

impl SchoolClosureData {
    pub fn new() -> Self {
        Self {
            dominant_school_closures: Vec::default(),
            overridden_school_closures: Vec::default(),
        }
    }

    pub fn add_dominant_school_closure(&mut self, region: RegionId) {
        self.dominant_school_closures.push(region);
    }

    pub fn add_overridden_school_closure(&mut self, region: RegionId) {
        self.overridden_school_closures.push(region);
    }

    pub fn remove_dominant_school_closure(&mut self, region: RegionId) {
        self.dominant_school_closures.retain(|g| g != &region);
    }

    pub fn remove_overridden_school_closure(&mut self, region: RegionId) {
        self.overridden_school_closures.retain(|g| g != &region);
    }

    pub fn get_dominant_school_closures(&self) -> &Vec<RegionId> {
        &self.dominant_school_closures
    }

    pub fn get_overridden_school_closures(&self) -> &Vec<RegionId> {
        &self.overridden_school_closures
    }
}

define_data_plugin!(
    SchoolClosureDataPlugin,
    SchoolClosureData,
    SchoolClosureData::default()
);

pub trait SchoolClosureContextExt:
    PluginContext
    + ContextEntitiesExt
    + ContextParametersExt
    + ContextTriggersExt
    + ContextSettingExt
    + ContextGeographyExt
{
    fn register_school_closure_itinerary_modifier(&mut self, region: RegionId) {
        let itinerary_modifier = define_virtual_school_closure_itinerary_modifier(region);
        let geography = self.get_property::<Region, Geography>(region);
        match geography {
            Geography::State(state) => {
                self.register_itinerary_modifier(SchoolState(Some(state)), itinerary_modifier);
            }
            Geography::County(state, county) => {
                self.register_itinerary_modifier(
                    SchoolCounty(Some((state, county))),
                    itinerary_modifier,
                );
            }
            Geography::CensusTract(state, county, tract) => {
                self.register_itinerary_modifier(
                    SchoolCensusTract(Some((state, county, tract))),
                    itinerary_modifier,
                );
            }
        }
    }
    fn remove_school_closure_itinerary_modifier(&mut self, region: RegionId) {
        let geography = self.get_property::<Region, Geography>(region);
        match geography {
            Geography::State(state) => {
                self.remove_itinerary_modifier_by_property(SchoolState(Some(state)));
            }
            Geography::County(state, county) => {
                self.remove_itinerary_modifier_by_property(SchoolCounty(Some((state, county))));
            }
            Geography::CensusTract(state, county, tract) => {
                self.remove_itinerary_modifier_by_property(SchoolCensusTract(Some((
                    state, county, tract,
                ))));
            }
        }
    }
    fn setup_school_closure_triggers(&mut self, start_time: f64, end_time: f64, region: RegionId) {
        let start_trigger =
            TimeTrigger::at_phase(start_time, ExecutionPhase::Last).emit_value(SchoolClosure {
                active: true,
                region,
            });
        let end_trigger =
            TimeTrigger::at_phase(end_time, ExecutionPhase::Last).emit_value(SchoolClosure {
                active: false,
                region,
            });

        self.register_trigger(start_trigger);
        self.register_trigger(end_trigger);
    }

    fn setup_school_closure_itinerary_modification(&mut self) {
        self.subscribe_to_event(move |context, event: SchoolClosure| {
            if event.active {
                context.handle_school_closure_start(event.region);
            } else {
                context.handle_school_closure_end(event.region);
            }
        });
    }

    fn handle_school_closure_start(&mut self, region: RegionId) {
        self.register_school_closure_itinerary_modifier(region);
        let mut overridden_flag = false;
        let mut new_overridden: Vec<RegionId> = Vec::new();
        {
            let data = self.get_data(SchoolClosureDataPlugin);
            let overlaps =
                self.filter_overlapping_regions(region, data.get_dominant_school_closures());
            if let Some(overlapping_regions) = overlaps {
                for overlapping_region in overlapping_regions {
                    if self.get_property::<Region, Geography>(region)
                        > self.get_property::<Region, Geography>(overlapping_region)
                    {
                        new_overridden.push(overlapping_region);
                    } else {
                        if !new_overridden.is_empty() {
                            panic!(
                                "Geographic overlap logical error: cannot add dominant school closure due to conflicting geography"
                            );
                        }
                        overridden_flag = true;
                        self.add_overridden_school_closure(region);
                        break;
                    }
                }
            }
        }
        for overridden_region in new_overridden.clone() {
            self.remove_dominant_school_closure(overridden_region);
        }
        for overridden_region in &new_overridden {
            self.add_overridden_school_closure(*overridden_region);
        }
        if !overridden_flag {
            self.add_dominant_school_closure(region);
        }
    }

    fn handle_school_closure_end(&mut self, region: RegionId) {
        self.remove_school_closure_itinerary_modifier(region);
        let is_dominant = {
            let data = self.get_data_mut(SchoolClosureDataPlugin);
            data.dominant_school_closures.contains(&region)
        };
        if is_dominant {
            self.remove_dominant_school_closure(region);
            let overlapping_regions = {
                let data = self.get_data(SchoolClosureDataPlugin);
                self.filter_overlapping_regions(region, &data.overridden_school_closures)
            };

            if let Some(regions) = overlapping_regions {
                let filtered_regions = self.filter_largest_nonoverlapping_regions(regions);
                for region in filtered_regions {
                    self.remove_overridden_school_closure(region);
                    self.add_dominant_school_closure(region);
                }
            }
        } else {
            let data = self.get_data_mut(SchoolClosureDataPlugin);
            if data.overridden_school_closures.contains(&region) {
                data.remove_overridden_school_closure(region);
            }
        }
    }

    fn add_dominant_school_closure(&mut self, region: RegionId) {
        let data = self.get_data_mut(SchoolClosureDataPlugin);
        data.add_dominant_school_closure(region);
    }

    fn add_overridden_school_closure(&mut self, region: RegionId) {
        let data = self.get_data_mut(SchoolClosureDataPlugin);
        data.add_overridden_school_closure(region);
    }

    fn remove_dominant_school_closure(&mut self, region: RegionId) {
        let data = self.get_data_mut(SchoolClosureDataPlugin);
        data.remove_dominant_school_closure(region);
    }

    fn remove_overridden_school_closure(&mut self, region: RegionId) {
        let data = self.get_data_mut(SchoolClosureDataPlugin);
        data.remove_overridden_school_closure(region);
    }

    fn is_dominant_school_closure(&self, region: RegionId) -> bool {
        let data = self.get_data(SchoolClosureDataPlugin);
        data.dominant_school_closures.contains(&region)
    }
}
impl SchoolClosureContextExt for Context {}

fn create_school_closure_from_record(
    context: &mut Context,
    school_closure_record: SchoolClosureRecord,
) -> Result<(), ModelError> {
    let region_id = school_closure_record.create_region_from_record(context)?;
    context.setup_school_closure_triggers(
        school_closure_record.start_time,
        school_closure_record.end_time,
        region_id,
    );
    Ok(())
}

fn read_school_closures_file(
    context: &mut Context,
    school_closure_file: PathBuf,
) -> Result<(), ModelError> {
    let mut reader = csv::Reader::from_path(school_closure_file)?;
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers()?.clone();

    while reader.read_byte_record(&mut raw_record)? {
        let record: SchoolClosureRecord = raw_record.deserialize(Some(&headers))?;
        create_school_closure_from_record(context, record)?;
    }
    Ok(())
}

fn load_school_closures(context: &mut Context) -> Result<(), ModelError> {
    let Params {
        school_closures, ..
    } = context.get_params();
    if school_closures.include {
        if let Some(filename) = &school_closures.filename {
            read_school_closures_file(context, filename.clone())?;
        } else {
            return Err(ModelError::ModelError(
                "School closures are turned on but no filename was provided.".to_string(),
            ));
        }
    }
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), ModelError> {
    load_school_closures(context)?;
    context.setup_school_closure_itinerary_modification();
    Ok(())
}

fn define_virtual_school_closure_itinerary_modifier(region: RegionId) -> ItineraryTransitionMatrix {
    let matrix = [
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
    ];
    let acceptance_function: Option<AcceptanceFunction> =
        Some(Arc::new(move |context, _person| {
            context.is_dominant_school_closure(region)
        }));
    define_itinerary_modifier(None, Some(matrix), acceptance_function)
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc};

    use ixa::HashMap;

    use crate::{
        Age,
        itinerary_manager::ContextItineraryModifierExt,
        parameters::{GlobalParams, SettingProperties},
        pop_reader::parser::parse_fips_school_id,
        settings::SettingCode,
    };

    use super::*;
    #[allow(dead_code)]
    fn make_school_id(school_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(school_id).unwrap().1)
    }

    fn setup() -> Context {
        let mut context = Context::new();
        let parameters = Params {
            // We need to specify an itinerary split here even though we don't draw people from
            // itineraries because `load_synth_population` calls `create_itinerary` for each person,
            // and that function requires an itinerary write function to be set.
            settings_properties: HashMap::from_iter(
                [
                    (SettingCategory::Home, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::School, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::Work, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::Community, SettingProperties { alpha: 0.0 }),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter([
                (SettingCategory::Home, 0.25),
                (SettingCategory::School, 0.25),
                (SettingCategory::Work, 0.25),
                (SettingCategory::Community, 0.25),
            ]),
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        crate::settings::init(&mut context).unwrap();
        context
    }

    #[test]
    fn test_setup_school_closure_triggers() {
        let mut context = setup();
        let school_closures_starts = Rc::new(RefCell::new(0));
        let school_closures_starts_clone: Rc<RefCell<usize>> = Rc::clone(&school_closures_starts);
        let school_closures_ends = Rc::new(RefCell::new(0));
        let school_closures_ends_clone: Rc<RefCell<usize>> = Rc::clone(&school_closures_ends);
        let r1 = context
            .add_entity(with!(Region, Geography::State(1)))
            .unwrap();
        context.setup_school_closure_triggers(1.0, 2.0, r1);
        context.setup_school_closure_itinerary_modification();
        context.subscribe_to_event(move |cxt, event: SchoolClosure| {
            if event.active && event.region == r1 {
                assert_eq!(cxt.get_current_time(), 1.0);
                *school_closures_starts_clone.borrow_mut() += 1;
            } else {
                assert_eq!(cxt.get_current_time(), 2.0);
                *school_closures_ends_clone.borrow_mut() += 1;
            }
        });
        context.add_plan_with_phase(3.0, ixa::Context::shutdown, ExecutionPhase::Last);
        context.execute();
        #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
        let observed_school_closures_starts = *school_closures_starts.borrow();
        let observed_school_closures_ends = *school_closures_ends.borrow();
        // school closures start on day 1
        // school closures end on day 2
        assert_eq!(observed_school_closures_starts, 1);
        assert_eq!(observed_school_closures_ends, 1);
    }

    #[test]
    fn test_setup_school_closure_itinerary_modification() {
        let mut context = setup();
        let school_code = make_school_id(b"16037960200002");
        let r1 = context
            .add_entity(with!(Region, Geography::State(school_code.0.state_code())))
            .unwrap();
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.setup_school_closure_itinerary_modification();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [None, None, Some(school_code), None],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );
        context.add_plan(1.0, move |context| {
            context.emit_event(SchoolClosure {
                active: true,
                region: r1,
            });
        });
        context.add_plan(2.0, move |context| {
            context.emit_event(SchoolClosure {
                active: false,
                region: r1,
            });
        });

        context.add_plan(0.0, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.3, 0.0, 0.5, 0.2]);
        });
        context.add_plan(1.5, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.8, 0.0, 0.0, 0.2]);
        });
        context.add_plan(3.0, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.3, 0.0, 0.5, 0.2]);
        });
        context.execute();
    }

    #[test]
    fn test_overlapping_school_closures() {
        let mut context = setup();
        context.setup_school_closure_itinerary_modification();
        let school_code = make_school_id(b"16037960200002");
        let r1 = context
            .add_entity(with!(Region, Geography::State(school_code.0.state_code())))
            .unwrap();
        let r2 = context
            .add_entity(with!(
                Region,
                Geography::County(school_code.0.state_code(), school_code.0.county_code())
            ))
            .unwrap();
        let r3 = context
            .add_entity(with!(
                Region,
                Geography::CensusTract(
                    school_code.0.state_code(),
                    school_code.0.county_code(),
                    school_code.0.census_tract_code()
                )
            ))
            .unwrap();
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [None, None, Some(school_code), None],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        context.add_plan(1.0, move |context| {
            context.emit_event(SchoolClosure {
                active: true,
                region: r3,
            });
        });
        context.add_plan(2.0, move |context| {
            context.emit_event(SchoolClosure {
                active: true,
                region: r1,
            });
        });
        context.add_plan(3.0, move |context| {
            context.emit_event(SchoolClosure {
                active: true,
                region: r2,
            });
        });
        context.add_plan(4.0, move |context| {
            context.emit_event(SchoolClosure {
                active: false,
                region: r1,
            });
        });
        context.add_plan(5.0, move |context| {
            context.emit_event(SchoolClosure {
                active: false,
                region: r3,
            });
        });
        context.add_plan(6.0, move |context| {
            context.emit_event(SchoolClosure {
                active: false,
                region: r2,
            });
        });

        context.add_plan(0.0, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert_eq!(data.get_dominant_school_closures(), &Vec::new());
            assert_eq!(data.get_overridden_school_closures(), &Vec::new());
        });
        context.add_plan(1.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert_eq!(data.get_dominant_school_closures(), &vec![r3]);
            assert_eq!(data.get_overridden_school_closures(), &Vec::new());
        });
        context.add_plan(2.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert_eq!(data.get_dominant_school_closures(), &vec![r1]);
            assert_eq!(data.get_overridden_school_closures(), &vec![r3]);
        });
        context.add_plan(3.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert_eq!(data.get_dominant_school_closures(), &vec![r1]);
            assert_eq!(data.get_overridden_school_closures(), &vec![r3, r2]);
        });
        context.add_plan(4.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert_eq!(data.get_dominant_school_closures(), &vec![r2]);
            assert_eq!(data.get_overridden_school_closures(), &vec![r3]);
        });
        context.add_plan(5.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert_eq!(data.get_dominant_school_closures(), &vec![r2]);
            assert_eq!(data.get_overridden_school_closures(), &Vec::new());
        });
        context.add_plan(6.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert_eq!(data.get_dominant_school_closures(), &Vec::new());
            assert_eq!(data.get_overridden_school_closures(), &Vec::new());
        });
        context.execute();
    }
}
