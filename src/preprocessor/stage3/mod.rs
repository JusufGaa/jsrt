/// Stage 3: builds on the stage-2 analysis (`Stage2`) and turns the
/// program into the code chunks the worker will compile and run.
mod stage;

pub use stage::Stage3;
