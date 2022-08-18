use model::CfdEvent;

impl crate::CfdAggregate for model::Cfd {
    type CtorArgs = ();

    // TODO(restioson): separate dlc loading
    fn new(_: Self::CtorArgs, cfd: crate::Cfd) -> Self {
        model::Cfd::new(cfd.order, cfd.dlc)
    }

    fn apply(self, event: CfdEvent) -> Self {
        self.apply(event)
    }

    fn version(&self) -> u32 {
        self.version()
    }
}
