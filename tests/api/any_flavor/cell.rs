use std::sync::Arc;

use pretty_assertions::assert_eq;

use super::flavor;
use nosy::{Listen as _, Log, Source as _};

#[test]
fn cell_and_source_debug() {
    let cell = flavor::Cell::<Vec<&'static str>>::new(vec!["hi"]);
    let source = cell.as_source();
    assert_eq!(
        format!("{cell:#?}"),
        indoc::indoc! {
            r#"Cell {
                value: [
                    "hi",
                ],
                owners: 2,
                listeners: 0,
            }"#
        }
    );
    // Note we can't print `owners` because that information isn't carried along,
    // unfortunately.
    assert_eq!(
        format!("{source:#?}"),
        indoc::indoc! {
           r#"CellSource {
                value: [
                    "hi",
                ],
                listeners: 0,
            }"#
        }
    );
}

#[test]
fn cell_usage() {
    let cell = flavor::Cell::<i32>::new(0i32);

    let s = cell.as_source();
    let log = Log::new();
    s.listen(log.listener());

    assert_eq!(log.drain(), vec![]);
    cell.set(1);
    assert_eq!(1, s.get());
    assert_eq!(log.drain(), vec![()]);
}

#[test]
fn cell_source_clone() {
    let cell = flavor::Cell::<i32>::new(0i32);
    let s = cell.as_source();
    let s = s.clone();
    cell.set(1);
    assert_eq!(s.get(), 1);
}

/// Dropping a [`Cell`] should drop all its listeners,
/// because it is then impossible for them to receive any more messages.
#[test]
fn dropping_cell_drops_listeners() {
    #[derive(Debug)]
    struct DropDetector(Arc<()>);
    impl nosy::Listener<()> for DropDetector {
        fn receive(&self, _: &[()]) -> bool {
            true
        }
    }

    let cell = flavor::Cell::<i32>::new(0i32);
    let source = cell.as_source();
    let detector = DropDetector(Arc::new(()));
    let weak = Arc::downgrade(&detector.0);
    source.listen(detector);

    assert_eq!(weak.strong_count(), 1);
    drop(cell);
    assert_eq!(weak.strong_count(), 0);
}
