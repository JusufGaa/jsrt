use crate::preprocessor::stage2::Stage2;
use crate::preprocessor::Stage;

/// Stage 3: takes the stage-2 analysis (functions, purity, globals) and
/// produces the code chunks handed over to the worker.
pub struct Stage3 {
    stage2: Stage2,
}

impl Stage3 {
    /// The underlying stage-2 analysis.
    pub fn analysis(&self) -> &Stage2 {
        &self.stage2
    }
}

impl From<Stage2> for Stage3 {
    fn from(stage2: Stage2) -> Self {
        Self { stage2 }
    }
}

impl Stage for Stage3 {
    fn stage(&self) -> u8 {
        3
    }

    fn get_code(&self) -> impl Iterator<Item = &str> {
        // TODO(stage3): replace with the actual stage-3 transformation
        // (chunking based on the stage-2 function/purity analysis).
        self.stage2.get_code()
    }
}
