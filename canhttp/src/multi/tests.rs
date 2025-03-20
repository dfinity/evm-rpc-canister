mod reduce_with_equality {
    use crate::multi::ReductionError;
    use crate::MultiResults;

    #[test]
    #[should_panic(expected = "MultiResults is empty")]
    fn should_panic_when_empty() {
        let empty: MultiResults<String, String, String> = MultiResults::default();
        let _panic = empty.reduce_with_equality();
    }

    #[test]
    fn should_be_inconsistent_results() {
        fn check_inconsistent_error(results: MultiResults<u8, &str, &str>) {
            let reduced = results.clone().reduce_with_equality();
            assert_eq!(reduced, Err(ReductionError::InconsistentResults(results)))
        }

        // different errors
        check_inconsistent_error(MultiResults::from_non_empty_iter(vec![
            (0_u8, Err("reject")),
            (1, Err("transient")),
        ]));
        // different ok results
        check_inconsistent_error(MultiResults::from_non_empty_iter(vec![
            (0_u8, Ok("hello")),
            (1, Ok("world")),
        ]));
        // mix of errors and ok results
        check_inconsistent_error(MultiResults::from_non_empty_iter(vec![
            (0_u8, Ok("same")),
            (1, Err("transient")),
            (2, Ok("same")),
        ]));
    }

    #[test]
    fn should_be_consistent_error() {
        fn check_consistent_error(results: MultiResults<u8, &str, &str>, expected_error: &str) {
            let reduced = results.reduce_with_equality();
            assert_eq!(
                reduced,
                Err(ReductionError::ConsistentError(expected_error))
            )
        }

        check_consistent_error(
            MultiResults::from_non_empty_iter(vec![(0_u8, Err("error"))]),
            "error",
        );
        check_consistent_error(
            MultiResults::from_non_empty_iter(vec![(0_u8, Err("error")), (1, Err("error"))]),
            "error",
        );
    }

    #[test]
    fn should_be_consistent_result() {
        fn check_consistent_result(results: MultiResults<u8, &str, &str>, expected_result: &str) {
            let reduced = results.reduce_with_equality();
            assert_eq!(reduced, Ok(expected_result))
        }

        check_consistent_result(
            MultiResults::from_non_empty_iter(vec![(1, Ok("same"))]),
            "same",
        );
        check_consistent_result(
            MultiResults::from_non_empty_iter(vec![(0_u8, Ok("same")), (1, Ok("same"))]),
            "same",
        );
    }
}
