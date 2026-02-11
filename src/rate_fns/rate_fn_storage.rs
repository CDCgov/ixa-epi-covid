use crate::{
    natural_history_parameter_manager::{
        ContextNaturalHistoryParameterExt, NaturalHistoryParameterLibrary,
    },
    parameters::{ContextParametersExt, RateFnType},
};
use ixa::{
    Context, ContextRandomExt, IxaError, PersonId, PluginContext, define_data_plugin, define_rng,
};

use super::{ConstantRate, rate_fn::InfectiousnessRateFn};

define_rng!(InfectiousnessRng);

struct RateFnContainer {
    rates: Vec<Box<dyn InfectiousnessRateFn>>,
}

pub struct RateFn;

impl NaturalHistoryParameterLibrary for RateFn {
    fn library_size(&self, context: &Context) -> usize {
        context.get_data(RateFnPlugin).rates.len()
    }
}

define_data_plugin!(
    RateFnPlugin,
    RateFnContainer,
    RateFnContainer { rates: Vec::new() }
);

pub trait InfectiousnessRateExt: PluginContext + ContextNaturalHistoryParameterExt {
    fn add_rate_fn(&mut self, dist: impl InfectiousnessRateFn + 'static) {
        let container = self.get_data_mut(RateFnPlugin);
        container.rates.push(Box::new(dist));
    }

    fn get_person_rate_fn(&self, person_id: PersonId) -> &dyn InfectiousnessRateFn {
        let id = self.get_parameter_id(RateFn, person_id);
        self.get_data(RateFnPlugin).rates[id].as_ref()
    }
}
impl InfectiousnessRateExt for Context {}

#[allow(clippy::missing_panics_doc)]
/// Turn the information specified in the global parameter `infectiousness_rate_fn` into actual
/// infectiousness rate functions for the simulation.
/// # Errors
/// - If the parameters used to specify the rate functions are invalid
pub fn load_rate_fns(context: &mut Context) -> Result<(), IxaError> {
    let rate_of_infection = context.get_params().infectiousness_rate_fn.clone();

    match rate_of_infection {
        RateFnType::Constant { rate, duration } => {
            context.add_rate_fn(ConstantRate::new(rate, duration)?);
        }
    }

    context.register_parameter_id_assigner(RateFn, |context, _person_id| {
        let library_size = RateFn.library_size(context);
        context.sample_range(InfectiousnessRng, 0..library_size)
    })?;
    Ok(())
}

#[cfg(test)]
mod test {
    use crate::parameters::{GlobalParams, Params};

    use super::*;
    use ixa::assert_almost_eq;
    use ixa::{Context, ContextGlobalPropertiesExt, ContextPeopleExt};

    struct TestRateFn;

    impl InfectiousnessRateFn for TestRateFn {
        fn rate(&self, _t: f64) -> f64 {
            1.0
        }
        fn cum_rate(&self, _t: f64) -> f64 {
            1.0
        }
        fn inverse_cum_rate(&self, _events: f64) -> Option<f64> {
            Some(1.0)
        }
        fn infection_duration(&self) -> f64 {
            1.0
        }
    }

    fn init_context() -> Context {
        let mut context = Context::new();
        context.init_random(0);
        context
            .register_parameter_id_assigner(RateFn, |context, _person_id| {
                let container = context.get_data(RateFnPlugin);
                let len = container.rates.len();
                context.sample_range(InfectiousnessRng, 0..len)
            })
            .unwrap();
        context
    }

    #[test]
    fn test_add_rate_fn_and_get_random() {
        let mut context = init_context();
        let person = context.add_person(()).unwrap();

        let rate_fn = TestRateFn {};
        context.add_rate_fn(rate_fn);
        let rate_fns = context.get_data(RateFnPlugin);
        assert_eq!(rate_fns.rates.len(), 1);

        assert_almost_eq!(context.get_person_rate_fn(person).rate(0.0), 1.0, 0.0);
    }

    #[test]
    fn test_load_rate_functions_constant() {
        let mut context = Context::new();
        let parameters = Params {
            infectiousness_rate_fn: RateFnType::Constant {
                rate: 1.0,
                duration: 5.0,
            },
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        let rate_fns = context.get_data(RateFnPlugin);
        assert_eq!(rate_fns.rates.len(), 1);
        let rate_fn = rate_fns.rates[0].as_ref();
        assert_almost_eq!(rate_fn.rate(0.0), 1.0, 0.0);
        assert_almost_eq!(rate_fn.rate(5.1), 0.0, 0.0);
        assert_almost_eq!(rate_fn.infection_duration(), 5.0, 0.0);
    }
}
