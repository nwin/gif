pub trait Parameter<Object> {
    fn set_param(self, &mut Object);
}

pub trait HasParameters: Sized {
    fn set<T: Parameter<Self>>(&mut self, value: T) -> &mut Self {
        value.set_param(self);
        self
    }
}