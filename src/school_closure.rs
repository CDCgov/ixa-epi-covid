use std::{cmp::Ordering, path::PathBuf};

use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    itinerary_modifiers::{ItineraryTransitionMatrix, define_itinerary_modifier},
    population_loader::SchoolId,
    settings::{ContextSettingExt, Itinerary, Person, SettingCategory},
};
use ixa::{
    ExecutionPhase, IxaEvent, csv, impl_derived_property,
    prelude::*,
    triggers::{ContextTriggersExt, TimeTrigger, TriggerCriterion},
};
use serde::{Deserialize, Serialize};
use strum::IntoDiscriminant;
use strum::{EnumDiscriminants, EnumIter, FromRepr, IntoStaticStr};

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

#[derive(
    Copy,
    Clone,
    PartialEq,
    Debug,
    Deserialize,
    Serialize,
    Eq,
    Hash,
    FromRepr,
    EnumIter,
    EnumDiscriminants,
)]
#[strum_discriminants(name(GeographyType))]
#[strum_discriminants(derive(PartialOrd, Ord, Hash))]
#[strum_discriminants(derive(IntoStaticStr), repr(u8))]
pub enum Geography {
    CensusTract(u8, u16, u32),
    County(u8, u16),
    State(u8),
}

impl Geography {
    fn geography_type(&self) -> GeographyType {
        GeographyType::from(*self)
    }

    fn geography_type_u8(&self) -> u8 {
        self.geography_type() as u8
    }
}

impl PartialOrd for Geography {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Geography {
    fn cmp(&self, other: &Self) -> Ordering {
        self.geography_type_u8().cmp(&other.geography_type_u8())
    }
}

fn filter_largest_geographies(geographies: Vec<Geography>) -> Vec<Geography> {
    let Some(max_kind) = geographies.iter().map(|g| g.discriminant() as u8).max() else {
        return Vec::new();
    };

    geographies
        .into_iter()
        .filter(|g| (g.discriminant() as u8) == max_kind)
        .collect()
}

fn overlaps(a: &Geography, b: &Geography) -> bool {
    match (a, b) {
        // Same type
        (Geography::State(s1), Geography::State(s2)) => s1 == s2,
        (Geography::County(s1, c1), Geography::County(s2, c2)) => s1 == s2 && c1 == c2,
        (Geography::CensusTract(s1, c1, t1), Geography::CensusTract(s2, c2, t2)) => {
            s1 == s2 && c1 == c2 && t1 == t2
        }

        // State contains County
        (Geography::State(s), Geography::County(s2, _))
        | (Geography::County(s2, _), Geography::State(s)) => s == s2,

        // State contains CensusTract
        (Geography::State(s), Geography::CensusTract(s2, _, _))
        | (Geography::CensusTract(s2, _, _), Geography::State(s)) => s == s2,

        // County contains CensusTract
        (Geography::County(s1, c1), Geography::CensusTract(s2, c2, _))
        | (Geography::CensusTract(s2, c2, _), Geography::County(s1, c1)) => s1 == s2 && c1 == c2,
    }
}

fn check_overlap(
    new_geography: Geography,
    existing_geographies: &[Geography],
) -> Option<Vec<Geography>> {
    let overlaps: Vec<Geography> = existing_geographies
        .iter()
        .filter(|g| overlaps(&new_geography, g))
        .cloned()
        .collect();

    if overlaps.is_empty() {
        None
    } else {
        Some(overlaps)
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct SchoolClosureRecord {
    pub geography: Geography,
    pub start_time: f64,
    pub end_time: f64,
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
    geography: Geography,
}

#[derive(Default)]
pub struct SchoolClosureData {
    dominant_school_closures: Vec<Geography>,
    overridden_school_closures: Vec<Geography>,
}

impl SchoolClosureData {
    pub fn new() -> Self {
        Self {
            dominant_school_closures: Vec::default(),
            overridden_school_closures: Vec::default(),
        }
    }

    pub fn add_dominant_school_closure(&mut self, geography: Geography) {
        self.dominant_school_closures.push(geography);
    }

    pub fn add_overridden_school_closure(&mut self, geography: Geography) {
        self.overridden_school_closures.push(geography);
    }

    pub fn remove_dominant_school_closure(&mut self, geography: Geography) {
        self.dominant_school_closures.retain(|g| g != &geography);
    }

    pub fn remove_overridden_school_closure(&mut self, geography: Geography) {
        self.overridden_school_closures.retain(|g| g != &geography);
    }

    pub fn get_dominant_school_closures(&self) -> &Vec<Geography> {
        &self.dominant_school_closures
    }

    pub fn get_overridden_school_closures(&self) -> &Vec<Geography> {
        &self.overridden_school_closures
    }
}

define_data_plugin!(
    SchoolClosureDataPlugin,
    SchoolClosureData,
    SchoolClosureData::default()
);

pub trait SchoolClosureContextExt:
    PluginContext + ContextEntitiesExt + ContextParametersExt + ContextTriggersExt + ContextSettingExt
{
    fn setup_school_closure_triggers(
        &mut self,
        start_time: f64,
        end_time: f64,
        geography: Geography,
    ) {
        let start_trigger =
            TimeTrigger::at_phase(start_time, ExecutionPhase::Last).emit_value(SchoolClosure {
                active: true,
                geography,
            });
        let end_trigger =
            TimeTrigger::at_phase(end_time, ExecutionPhase::Last).emit_value(SchoolClosure {
                active: false,
                geography,
            });

        self.register_trigger(start_trigger);
        self.register_trigger(end_trigger);
    }

    fn setup_school_closure_itinerary_modification(
        &mut self,
        itinerary_modifier: ItineraryTransitionMatrix,
    ) {
        self.subscribe_to_event(move |context, event: SchoolClosure| {
            if event.active {
                context.handle_school_closure_start(event.geography, itinerary_modifier);
            } else {
                context.handle_school_closure_end(event.geography, itinerary_modifier);
            }
        });
    }

    fn handle_school_closure_start(
        &mut self,
        geography: Geography,
        itinerary_modifier: ItineraryTransitionMatrix,
    ) {
        let mut overridden_flag = false;
        let mut new_overridden: Vec<Geography> = Vec::new();
        {
            let data = self.get_data_mut(SchoolClosureDataPlugin);
            let overlaps = check_overlap(geography, &data.dominant_school_closures);
            if let Some(overlapping_geographies) = overlaps {
                println!(
                    "geography: {:?}, overlapping_geographies: {:?}",
                    geography, overlapping_geographies
                );
                for overlapping_geography in overlapping_geographies {
                    if geography > overlapping_geography {
                        new_overridden.push(overlapping_geography);
                    } else {
                        if !new_overridden.is_empty() {
                            panic!(
                                "Geographic overlap logical error: cannot add dominant school closure due to conflicting geography"
                            );
                        }
                        overridden_flag = true;
                        data.add_overridden_school_closure(geography);
                        break;
                    }
                }
            }
        }
        for overridden_geography in new_overridden.clone() {
            self.remove_dominant_school_closure(overridden_geography);
        }
        for overridden_geography in &new_overridden {
            self.add_overridden_school_closure(*overridden_geography);
        }
        if !overridden_flag {
            self.add_dominant_school_closure(geography, itinerary_modifier);
        }
    }

    fn handle_school_closure_end(
        &mut self,
        geography: Geography,
        itinerary_modifier: ItineraryTransitionMatrix,
    ) {
        let is_dominant = {
            let data = self.get_data_mut(SchoolClosureDataPlugin);
            data.dominant_school_closures.contains(&geography)
        };
        if is_dominant {
            self.remove_dominant_school_closure(geography);
            let overlapping_geographies = {
                let data = self.get_data_mut(SchoolClosureDataPlugin);
                check_overlap(geography, &data.overridden_school_closures)
            };

            if let Some(geographies) = overlapping_geographies {
                let filtered_geographies = filter_largest_geographies(geographies);
                for geography in filtered_geographies {
                    self.remove_overridden_school_closure(geography);
                    self.add_dominant_school_closure(geography, itinerary_modifier);
                }
            }
        } else {
            let data = self.get_data_mut(SchoolClosureDataPlugin);
            if data.overridden_school_closures.contains(&geography) {
                data.remove_overridden_school_closure(geography);
            }
        }
    }

    fn add_dominant_school_closure(
        &mut self,
        geography: Geography,
        itinerary_modifier: ItineraryTransitionMatrix,
    ) {
        let data = self.get_data_mut(SchoolClosureDataPlugin);
        data.add_dominant_school_closure(geography);
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

    fn add_overridden_school_closure(&mut self, geography: Geography) {
        let data = self.get_data_mut(SchoolClosureDataPlugin);
        data.add_overridden_school_closure(geography);
    }

    fn remove_dominant_school_closure(&mut self, geography: Geography) {
        let data = self.get_data_mut(SchoolClosureDataPlugin);
        data.remove_dominant_school_closure(geography);
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

    fn remove_overridden_school_closure(&mut self, geography: Geography) {
        let data = self.get_data_mut(SchoolClosureDataPlugin);
        data.remove_overridden_school_closure(geography);
    }
}
impl SchoolClosureContextExt for Context {}

fn create_school_closure_from_record(
    context: &mut Context,
    school_closure_record: SchoolClosureRecord,
) -> Result<(), ModelError> {
    context.setup_school_closure_triggers(
        school_closure_record.start_time,
        school_closure_record.end_time,
        school_closure_record.geography,
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
    let school_closure_itinerary_modifier = define_virtual_school_closure_itinerary_modifier();
    load_school_closures(context)?;
    context.setup_school_closure_itinerary_modification(school_closure_itinerary_modifier);
    Ok(())
}

fn define_virtual_school_closure_itinerary_modifier() -> ItineraryTransitionMatrix {
    let weekend_matrix = [
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
    ];

    define_itinerary_modifier(None, Some(weekend_matrix))
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
    fn test_overlap() {
        let g1 = Geography::State(1);
        let g2 = Geography::County(1, 2);
        let g3 = Geography::CensusTract(1, 2, 3);
        let g4 = Geography::State(2);
        assert!(overlaps(&g1, &g2));
        assert!(overlaps(&g1, &g3));
        assert!(overlaps(&g2, &g3));
        assert!(!overlaps(&g1, &g4));
        assert!(!overlaps(&g2, &g4));
        assert!(!overlaps(&g3, &g4));
    }

    #[test]
    fn test_check_overlap() {
        let g1 = Geography::State(1);
        let g2 = Geography::County(1, 2);
        let g3 = Geography::CensusTract(1, 2, 3);
        let g4 = Geography::State(2);
        let existing_geographies = vec![g1, g2, g3, g4];
        assert_eq!(
            check_overlap(g1, &existing_geographies),
            Some(vec![g1, g2, g3])
        );
        assert_eq!(
            check_overlap(g2, &existing_geographies),
            Some(vec![g1, g2, g3])
        );
        assert_eq!(
            check_overlap(g3, &existing_geographies),
            Some(vec![g1, g2, g3])
        );
        assert_eq!(check_overlap(g4, &existing_geographies), Some(vec![g4]));
    }

    #[test]
    fn test_filter_largest_geographies() {
        let g1 = Geography::State(1);
        let g2 = Geography::County(1, 2);
        let g3 = Geography::CensusTract(1, 2, 3);
        let g4 = Geography::State(2);
        let geographies = vec![g1, g2, g3, g4];
        let geographies2 = vec![g2, g3];
        let filtered = filter_largest_geographies(geographies);
        assert_eq!(filtered, vec![g1, g4]);
        let filtered2 = filter_largest_geographies(geographies2);
        assert_eq!(filtered2, vec![g2]);
    }

    #[test]
    #[allow(clippy::nonminimal_bool)]
    fn test_geography_ordering() {
        let g1 = Geography::State(1);
        let g2 = Geography::County(1, 2);
        let g3 = Geography::CensusTract(1, 2, 3);
        let g4 = Geography::State(2);
        assert!(g1 > g2);
        assert!(g2 > g3);
        assert!(g1 > g3);
        assert!(!(g1 < g4) && !(g4 > g1));
    }

    #[test]
    fn test_setup_school_closure_triggers() {
        let mut context = setup();
        let school_closures_starts = Rc::new(RefCell::new(0));
        let school_closures_starts_clone: Rc<RefCell<usize>> = Rc::clone(&school_closures_starts);
        let school_closures_ends = Rc::new(RefCell::new(0));
        let school_closures_ends_clone: Rc<RefCell<usize>> = Rc::clone(&school_closures_ends);
        let itinerary_modifier = define_virtual_school_closure_itinerary_modifier();
        context.setup_school_closure_triggers(1.0, 2.0, Geography::State(1));
        context.setup_school_closure_itinerary_modification(itinerary_modifier);
        context.subscribe_to_event(move |cxt, event: SchoolClosure| {
            if event.active && event.geography == Geography::State(1) {
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
        let itinerary_modifier = define_virtual_school_closure_itinerary_modifier();
        let school_code = make_school_id(b"16037960200002");
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.setup_school_closure_itinerary_modification(itinerary_modifier);
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
                geography: Geography::State(school_code.0.state_code()),
            });
        });
        context.add_plan(2.0, move |context| {
            context.emit_event(SchoolClosure {
                active: false,
                geography: Geography::State(school_code.0.state_code()),
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
        let itinerary_modifier = define_virtual_school_closure_itinerary_modifier();
        context.setup_school_closure_itinerary_modification(itinerary_modifier);
        let school_code = make_school_id(b"16037960200002");
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [None, None, Some(school_code), None],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );

        let state_code = school_code.0.state_code();
        let county_code = school_code.0.county_code();
        let tract_code = school_code.0.census_tract_code();
        let g_state = Geography::State(state_code);
        let g_county = Geography::County(state_code, county_code);
        let g_tract = Geography::CensusTract(state_code, county_code, tract_code);

        context.add_plan(1.0, move |context| {
            context.emit_event(SchoolClosure {
                active: true,
                geography: g_tract,
            });
        });
        context.add_plan(2.0, move |context| {
            context.emit_event(SchoolClosure {
                active: true,
                geography: g_state,
            });
        });
        context.add_plan(3.0, move |context| {
            context.emit_event(SchoolClosure {
                active: true,
                geography: g_county,
            });
        });
        context.add_plan(4.0, move |context| {
            context.emit_event(SchoolClosure {
                active: false,
                geography: g_state,
            });
        });
        context.add_plan(5.0, move |context| {
            context.emit_event(SchoolClosure {
                active: false,
                geography: g_tract,
            });
        });
        context.add_plan(6.0, move |context| {
            context.emit_event(SchoolClosure {
                active: false,
                geography: g_county,
            });
        });

        context.add_plan(0.0, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert_eq!(data.get_dominant_school_closures(), &Vec::new());
            assert_eq!(data.get_overridden_school_closures(), &Vec::new());
        });
        context.add_plan(1.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert_eq!(
                data.get_dominant_school_closures(),
                &vec![Geography::CensusTract(state_code, county_code, tract_code)]
            );
            assert_eq!(data.get_overridden_school_closures(), &Vec::new());
        });
        context.add_plan(2.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert_eq!(
                data.get_dominant_school_closures(),
                &vec![Geography::State(state_code)]
            );
            assert_eq!(
                data.get_overridden_school_closures(),
                &vec![Geography::CensusTract(state_code, county_code, tract_code)]
            );
        });
        context.add_plan(3.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert_eq!(
                data.get_dominant_school_closures(),
                &vec![Geography::State(state_code)]
            );
            assert_eq!(
                data.get_overridden_school_closures(),
                &vec![
                    Geography::CensusTract(state_code, county_code, tract_code),
                    Geography::County(state_code, county_code)
                ]
            );
        });
        context.add_plan(4.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert_eq!(
                data.get_dominant_school_closures(),
                &vec![Geography::County(state_code, county_code)]
            );
            assert_eq!(
                data.get_overridden_school_closures(),
                &vec![Geography::CensusTract(state_code, county_code, tract_code)]
            );
        });
        context.add_plan(5.5, move |context| {
            let data = context.get_data(SchoolClosureDataPlugin);
            assert_eq!(
                data.get_dominant_school_closures(),
                &vec![Geography::County(state_code, county_code)]
            );
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
