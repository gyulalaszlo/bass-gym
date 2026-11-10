//! [`ThemeCache`]: a component theme that's rebuilt only when the tokens change.

use std::sync::Arc;

use crate::theme::{self, Tokens};

/// Holds a theme built from the current [`Tokens`] by `build`, and rebuilds it when the
/// display preferences change them. The owner of a theme tree's root keeps one of these
/// as a field and hands the theme (or parts of it) down by reference.
pub struct ThemeCache<T> {
    build: fn(&Tokens) -> T,
    built: Option<(Tokens, Arc<T>)>,
}

/// For themes with a `From<&Tokens>` (their usual constructor), so an owner can derive
/// `Default`.
impl<T: for<'a> From<&'a Tokens>> Default for ThemeCache<T> {
    fn default() -> Self {
        Self::new(|t| T::from(t))
    }
}

impl<T> ThemeCache<T> {
    pub const fn new(build: fn(&Tokens) -> T) -> Self {
        Self { build, built: None }
    }

    /// The theme for the current tokens. Cheap: a comparison and a pointer copy, unless
    /// the tokens changed.
    pub fn get(&mut self) -> Arc<T> {
        self.get_for(&theme::tokens())
    }

    /// The theme for `tokens`.
    pub fn get_for(&mut self, tokens: &Tokens) -> Arc<T> {
        match &self.built {
            Some((t, built)) if t == tokens => built.clone(),
            _ => {
                let built = Arc::new((self.build)(tokens));
                self.built = Some((*tokens, built.clone()));
                built
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::DAY;

    #[test]
    fn rebuilds_only_when_the_tokens_change() {
        let mut cache = ThemeCache::new(|t: &Tokens| t.bg());
        let ecam = Tokens::default();
        let day = Tokens {
            palette: DAY,
            ..ecam
        };
        let a = cache.get_for(&ecam);
        let b = cache.get_for(&ecam);
        assert!(Arc::ptr_eq(&a, &b));
        let c = cache.get_for(&day);
        assert_eq!(*c, DAY.bg.color());
        assert!(!Arc::ptr_eq(&a, &c));
    }
}
