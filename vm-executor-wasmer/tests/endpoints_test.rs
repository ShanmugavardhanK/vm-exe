mod common;

#[test]
fn instance_endpoints_empty() {
    let instance = common::test_instance(common::EMPTY_SC_WAT);
    assert_eq!(
        instance.get_exported_function_names(),
        vec!["init", "callBack"]
    );
}

#[test]
fn instance_endpoints_adder() {
    let instance = common::test_instance(common::ADDER_WAT);
    assert!(instance.has_function("add"));
    assert!(!instance.has_function("missingEndpoint"));
    assert_eq!(
        instance.get_exported_function_names(),
        vec!["init", "add", "getSum", "callBack"]
    );
}

#[test]
fn cache_round_trip_accepts_enveloped_cache() {
    let instance = common::test_instance(common::ADDER_WAT);
    let cache = instance.cache().unwrap();

    let executor = common::test_executor();
    let restored = executor
        .new_instance_from_cache(&cache, &common::DUMMY_COMPILATION_OPTIONS)
        .unwrap();

    assert!(restored.has_function("add"));
    assert!(!restored.has_function("missingEndpoint"));
}

#[test]
fn cache_round_trip_accepts_different_runtime_gas_limit() {
    let instance = common::test_instance(common::ADDER_WAT);
    let cache = instance.cache().unwrap();

    let executor = common::test_executor();
    let mut restore_options = common::DUMMY_COMPILATION_OPTIONS;
    restore_options.gas_limit = common::DUMMY_COMPILATION_OPTIONS.gas_limit + 10_000;
    let restored = executor
        .new_instance_from_cache(&cache, &restore_options)
        .unwrap();

    assert!(restored.has_function("add"));
    assert!(!restored.has_function("missingEndpoint"));
}

#[test]
fn cache_round_trip_rejects_modified_enveloped_cache() {
    let instance = common::test_instance(common::ADDER_WAT);
    let mut cache = instance.cache().unwrap();
    *cache.last_mut().unwrap() ^= 0xff;

    let executor = common::test_executor();
    let result = executor.new_instance_from_cache(&cache, &common::DUMMY_COMPILATION_OPTIONS);

    assert!(result.is_err());
}

#[test]
fn bad_init_param() {
    let instance = common::test_instance(common::BAD_INIT_PARAM);
    assert!(!instance.check_signatures());
}

#[test]
fn bad_init_result() {
    let instance = common::test_instance(common::BAD_INIT_RESULT);
    assert!(!instance.check_signatures());
}
