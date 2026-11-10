//! [`Emit`]: where a view puts the messages it sends during a frame.

/// Messages sent during one frame, in order. A thin wrapper over a `Vec`: one that stays
/// empty never allocates, which is the usual case.
#[derive(Debug)]
pub struct Emit<M>(Vec<M>);

impl<M> Default for Emit<M> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<M> Emit<M> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn send(&mut self, m: M) {
        self.0.push(m);
    }

    /// Sends each of a child's messages, wrapped by `f`: usually a variant of the
    /// parent's `Msg` (Elm's `Html.map`).
    pub fn extend_map<C>(&mut self, msgs: impl IntoIterator<Item = C>, f: impl FnMut(C) -> M) {
        self.0.extend(msgs.into_iter().map(f));
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn into_vec(self) -> Vec<M> {
        self.0
    }
}

impl<M> IntoIterator for Emit<M> {
    type Item = M;
    type IntoIter = std::vec::IntoIter<M>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extend_map_wraps_and_keeps_order() {
        #[derive(Debug, PartialEq)]
        enum Parent {
            Child(u8),
        }
        let mut out = Emit::new();
        out.extend_map(vec![1u8, 2], Parent::Child);
        assert_eq!(out.into_vec(), vec![Parent::Child(1), Parent::Child(2)]);
    }
}
