#[derive(Debug)]
pub enum Error {
    InvalidId,
    AlreadyConnected,
    CycleDetected,
    InvalidPortType,
    Lifetime,
    Graph,
}
