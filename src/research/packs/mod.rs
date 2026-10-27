//! Industry packs (research plan §4.3, D6, design.md §3.3): versioned TOML definitions,
//! the formula evaluator over facts and KPIs, and the loader with hot reload.

pub mod builtin;
pub mod def;
pub mod eval;
pub mod payload;

pub use builtin::{Loaded, Packs};
pub use def::{
    Binding, Cadence, Dimension, DimensionKind, Direction, Driver, Formula, Layer, Metric, Pack,
    PackError, PackSet, Period, ValidationError, CORE_ID,
};
pub use eval::{evaluate, Ctx, Gap, Outcome, Quarter, Source, Used, View};
