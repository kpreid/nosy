/// Breaks the listener rules for testing by recording batch boundaries.
#[derive(Debug)]
pub(crate) struct CaptureBatch<L>(pub L);

impl<M: Clone, L> nosy::Listener<M> for CaptureBatch<L>
where
    L: nosy::Listener<Vec<M>>,
{
    fn receive(&self, messages: &[M]) -> bool {
        self.0.receive(&[Vec::from(messages)])
    }
}
