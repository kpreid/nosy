use pretty_assertions::assert_eq;

use super::flavor;
use nosy::{Listen as _, Sink};

#[test]
fn constant_source_debug() {
    let source = flavor::constant(123);
    assert_eq!(format!("{source:?}"), "Constant(123)");
    assert_eq!(format!("{source:#?}"), "Constant(\n    123,\n)");
}

#[test]
fn constant_source_usage() {
    let sink: Sink<()> = Sink::new();
    let source = flavor::constant(123u64);
    source.listen(sink.listener()); // doesn't panic, doesn't do anything
    assert_eq!(source.get(), 123);
    assert_eq!(source.get(), 123);
    assert!(sink.drain().is_empty());
}
