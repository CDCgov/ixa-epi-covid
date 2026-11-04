use ixa::{prelude::*};

pub trait Modifier<M, K> {
    fn register(context: &mut Context, key: K, modifier: M);
    fn remove(context: &mut Context, key: K);
}

pub trait ContextModifierExt {
    fn register_modifier<F, M, K>(&mut self, key: K, modifier: M)
    where
        F: Modifier<M, K>;

    fn remove_modifier<F, M, K>(&mut self, key: K)
    where
        F: Modifier<M, K>;
}

impl ContextModifierExt for Context {
    fn register_modifier<F, M, K>(&mut self, key: K, modifier: M)
    where
        F: Modifier<M, K>,
    {
        F::register(self, key, modifier);
    }

    fn remove_modifier<F, M, K>(&mut self, key: K)
    where
        F: Modifier<M, K>,
    {
        F::remove(self, key)
    }
}
