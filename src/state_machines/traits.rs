pub trait StateTrasition {}

pub trait ArcMutexState<T: StateTrasition> {
    fn transition(&self, next: T) -> Result<(), super::FsmError>;
}
