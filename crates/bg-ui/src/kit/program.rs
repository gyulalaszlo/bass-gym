//! Tier 3: [`Program`], the root of a screen.

use std::collections::VecDeque;

use super::Emit;

/// The root of a screen (an exercise, a window). Unlike a component, it doesn't emit to
/// a parent: it has effects instead, which run against the outside world.
///
/// One frame of a program:
/// 1. `sync` (the program's own code) reads the outside world (engine, worker state,
///    settings) and turns what changed into `Msg`s. It's the only part that reads it.
/// 2. Each view region shows its props and sends `Msg`s, and children's emissions arrive
///    wrapped in `Msg`s too.
/// 3. [`dispatch`] runs `update` on each message. `update` is pure: it changes the model
///    and sends `Effect`s.
/// 4. `run` carries out each effect (the only code that drives the engine). What
///    comes of it comes back as more `Msg`s, handled in the same dispatch.
pub trait Program {
    type Model;
    type Msg;
    type Effect;
    /// What effects run against, e.g. the audio engine.
    type Ctx: ?Sized;

    fn update(model: &mut Self::Model, msg: Self::Msg, fx: &mut Emit<Self::Effect>);

    fn run(effect: Self::Effect, ctx: &mut Self::Ctx, out: &mut Emit<Self::Msg>);
}

/// Most messages one dispatch handles; past this a loop of effects and messages is
/// assumed to be a bug, and the rest are dropped.
const MAX_MESSAGES: usize = 256;

/// Updates `model` with `msgs` and runs the effects, until no messages are left. Returns
/// whether there were any messages (so the caller can request a repaint).
pub fn dispatch<P: Program>(
    model: &mut P::Model,
    ctx: &mut P::Ctx,
    msgs: impl IntoIterator<Item = P::Msg>,
) -> bool {
    let mut queue: VecDeque<P::Msg> = msgs.into_iter().collect();
    let any = !queue.is_empty();
    let mut handled = 0;
    while let Some(msg) = queue.pop_front() {
        handled += 1;
        if handled > MAX_MESSAGES {
            log::error!("kit::dispatch: over {MAX_MESSAGES} messages, dropping the rest");
            debug_assert!(false, "message loop");
            break;
        }
        let mut fx = Emit::new();
        P::update(model, msg, &mut fx);
        for effect in fx {
            let mut out = Emit::new();
            P::run(effect, ctx, &mut out);
            queue.extend(out);
        }
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Counts to a target through an effect that echoes back.
    struct Counter;

    impl Program for Counter {
        type Model = Vec<u32>;
        type Msg = u32;
        type Effect = u32;
        type Ctx = Vec<u32>;

        fn update(model: &mut Vec<u32>, msg: u32, fx: &mut Emit<u32>) {
            model.push(msg);
            if msg < 3 {
                fx.send(msg + 1);
            }
        }

        fn run(effect: u32, ran: &mut Vec<u32>, out: &mut Emit<u32>) {
            ran.push(effect);
            out.send(effect);
        }
    }

    #[test]
    fn effects_feed_back_until_done() {
        let (mut model, mut ran) = (Vec::new(), Vec::new());
        assert!(dispatch::<Counter>(&mut model, &mut ran, [1]));
        assert_eq!(model, vec![1, 2, 3]);
        assert_eq!(ran, vec![2, 3]);
        assert!(!dispatch::<Counter>(&mut model, &mut ran, []));
    }
}
