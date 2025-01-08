use std::sync::Arc;

use nosy::{Listen as _, Sink, Source as _};

use super::flavor;

// TODO: put this in the flavor modules?
type CellSource<T> = Arc<nosy::CellSource<T, flavor::DynListener<()>>>;

mod constant {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn debug() {
        let source = flavor::constant(123);
        assert_eq!(format!("{source:?}"), "Constant(123)");
        assert_eq!(format!("{source:#?}"), "Constant(\n    123,\n)");
    }

    #[test]
    fn usage() {
        let sink: Sink<()> = Sink::new();
        let source = flavor::constant(123u64);
        source.listen(sink.listener()); // doesn't panic, doesn't do anything
        assert_eq!(source.get(), 123);
        assert_eq!(source.get(), 123);
        assert!(sink.drain().is_empty());
    }
}

mod flatten {
    use super::*;

    #[test]
    fn usage() {
        let cell_1 = flavor::Cell::new(10);
        let cell_2 = flavor::Cell::new(20);
        let cell_of_source = flavor::Cell::new(cell_1.as_source());

        let flatten: nosy::Flatten<CellSource<CellSource<i8>>> =
            cell_of_source.as_source().flatten();
        let sink: Sink<()> = Sink::new();
        flatten.listen(sink.listener());

        // Initial outcomes: correct value, and no notifications.
        assert_eq!(flatten.get(), 10, "initial get");
        assert_eq!(sink.drain().len(), 0, "initial notif");

        // Change the first source.
        cell_1.set(11);
        assert_eq!(sink.drain().len(), 1, "cell_1 notif");
        assert_eq!(flatten.get(), 11, "cell_1 new value");
        assert_eq!(sink.drain().len(), 0, "cell_1 no notif");

        // Change sources.
        cell_of_source.set(cell_2.as_source());
        assert_eq!(sink.drain().len(), 1, "source notif");
        assert_eq!(flatten.get(), 20, "source new value");
        assert_eq!(sink.drain().len(), 0, "source no notif");

        // Change the second source.
        cell_2.set(21);
        assert_eq!(sink.drain().len(), 1, "cell_2 notif");
        assert_eq!(flatten.get(), 21, "cell_2 new value");
        assert_eq!(sink.drain().len(), 0, "cell_2 no notif");

        // Change the first source and expect no notification.
        cell_1.set(11);
        assert_eq!(sink.drain().len(), 0, "cell_1 obsolete no notif");
        assert_eq!(flatten.get(), 21, "cell_1 obsolete no value change");
    }

    // TODO: test weak-ref/leak-related behaviors
}

mod map {
    use super::*;

    #[test]
    fn debug() {
        fn some_map_fn(_: i32) -> i32 {
            456
        }
        let mapped = flavor::constant(123).map(some_map_fn);

        let fn_name = std::any::type_name_of_val(&some_map_fn);
        assert_eq!(
            format!("{mapped:?}"),
            format!("Map {{ function: {fn_name}, source: Constant(123) }}")
        );
    }

    #[test]
    fn usage() {
        let cell = flavor::Cell::new(1);
        let mapped: nosy::Map<CellSource<u8>, _> = cell.as_source().map(|x: u8| u16::from(x) * 100);
        let sink: Sink<()> = Sink::new();
        mapped.listen(sink.listener());

        assert_eq!(mapped.get(), 100u16);
        assert_eq!(sink.drain().len(), 0);

        cell.set(2);
        assert_eq!(sink.drain().len(), 1);
        assert_eq!(mapped.get(), 200u16);
        assert_eq!(sink.drain().len(), 0);
    }
}
