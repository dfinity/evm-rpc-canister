mod reduce_with_equality {
    use crate::multi::{MultiResults, ReduceWithEquality, ReductionError};

    #[test]
    #[should_panic(expected = "MultiResults is empty")]
    fn should_panic_when_empty() {
        let empty: MultiResults<String, String, String> = MultiResults::default();
        let _panic = empty.reduce(ReduceWithEquality);
    }

    #[test]
    fn should_be_inconsistent_results() {
        fn check_inconsistent_error(results: MultiResults<u8, &str, &str>) {
            let reduced = results.clone().reduce(ReduceWithEquality);
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
        for inconsistent_result in [Ok("different"), Err("offline")] {
            for index in 0..4 {
                let mut results = [Ok("same"), Ok("same"), Ok("same"), Ok("same")];
                results[index] = inconsistent_result.clone();

                let [result_0, result_1, result_2, result_3] = results;

                check_inconsistent_error(MultiResults::from_non_empty_iter(vec![
                    (0_u8, result_0),
                    (1, result_1),
                    (2, result_2),
                    (3, result_3),
                ]));
            }
        }
    }

    #[test]
    fn should_be_consistent_error() {
        fn check_consistent_error(results: MultiResults<u8, &str, &str>, expected_error: &str) {
            let reduced = results.reduce(ReduceWithEquality);
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
            let reduced = results.reduce(ReduceWithEquality);
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

mod reduce_with_threshold {
    use crate::multi::{MultiResults, ReduceWithThreshold, ReductionError};

    #[test]
    fn should_get_consistent_result() {
        fn check_consistent_result(
            results: MultiResults<u8, &str, &str>,
            threshold: u8,
            expected_result: &str,
        ) {
            let reduced = results.reduce(ReduceWithThreshold::new(threshold));
            assert_eq!(reduced, Ok(expected_result));
        }

        // unanimous
        check_consistent_result(
            MultiResults::from_non_empty_iter(vec![
                (0_u8, Ok("same")),
                (1, Ok("same")),
                (2, Ok("same")),
                (3, Ok("same")),
            ]),
            3,
            "same",
        );

        // 3 out-of-4 ok
        for inconsistent_result in [Ok("different"), Err("offline")] {
            for index_inconsistent in 0..4_usize {
                let mut results = [Ok("same"), Ok("same"), Ok("same"), Ok("same")];
                results[index_inconsistent] = inconsistent_result.clone();
                let [result_0, result_1, result_2, result_3] = results;

                check_consistent_result(
                    MultiResults::from_non_empty_iter(vec![
                        (0_u8, result_0),
                        (1, result_1),
                        (2, result_2),
                        (3, result_3),
                    ]),
                    3,
                    "same",
                );
            }
        }
    }

    #[test]
    fn should_get_inconsistent_error() {
        use itertools::Itertools;

        fn check_inconsistent_result(results: MultiResults<u8, &str, &str>, threshold: u8) {
            let reduced = results.clone().reduce(ReduceWithThreshold::new(threshold));
            assert_eq!(reduced, Err(ReductionError::InconsistentResults(results)));
        }

        //not enough results
        check_inconsistent_result(MultiResults::from_non_empty_iter(vec![(0, Ok("same"))]), 2);
        check_inconsistent_result(
            MultiResults::from_non_empty_iter(vec![(0, Ok("same")), (1, Ok("same"))]),
            3,
        );

        // 2-out-of-4 ok
        let inconsistent_results = [Ok("different"), Err("offline")];
        for (inconsistent_res_1, inconsistent_res_2) in inconsistent_results
            .clone()
            .iter()
            .cartesian_product(inconsistent_results)
        {
            for indexes in (0..4_usize).permutations(2) {
                let mut results = [Ok("same"), Ok("same"), Ok("same"), Ok("same")];
                results[indexes[0]] = inconsistent_res_1.clone();
                results[indexes[1]] = inconsistent_res_2.clone();
                let [result_0, result_1, result_2, result_3] = results;

                check_inconsistent_result(
                    MultiResults::from_non_empty_iter(vec![
                        (0_u8, result_0),
                        (1, result_1),
                        (2, result_2),
                        (3, result_3),
                    ]),
                    3,
                );
            }
        }

        // 1-out-of-4 ok
        for ok_index in 0..4_usize {
            let mut results = [
                Err("offline"),
                Err("offline"),
                Err("offline"),
                Err("offline"),
            ];
            results[ok_index] = Ok("same");
            let [result_0, result_1, result_2, result_3] = results;

            check_inconsistent_result(
                MultiResults::from_non_empty_iter(vec![
                    (0_u8, result_0),
                    (1, result_1),
                    (2, result_2),
                    (3, result_3),
                ]),
                3,
            );
        }
    }

    #[test]
    fn should_get_consistent_error() {
        let results: MultiResults<_, &str, _> = MultiResults::from_non_empty_iter(vec![
            (0_u8, Err("offline")),
            (1, Err("offline")),
            (2, Err("offline")),
            (3, Err("offline")),
        ]);

        assert_eq!(
            results.reduce(ReduceWithThreshold::new(3)),
            Err(ReductionError::ConsistentError("offline"))
        )
    }
}
