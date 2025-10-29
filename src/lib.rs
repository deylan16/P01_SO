pub mod control;
pub mod errors;
pub mod handlers;

// Re-exportar tipos principales para uso en tests
pub use control::{Job, SharedState, Task, WorkerInfo};
pub use errors::ResponseMeta;
