use super::*;

#[test]
fn query_cannot_supply_foreign_authority_or_an_unbounded_result_count() {
    let parsed: Query = serde_json::from_value(json!({"query":"Café"})).unwrap();
    assert_eq!(parsed.limit, 10);
    assert!(parsed.cursor.is_none());
    for field in [
        "space", "space_id", "source", "serving", "agent", "actor", "ref",
    ] {
        let mut args = json!({"query":"Café"});
        args[field] = "foreign-authority".into();
        assert!(serde_json::from_value::<Query>(args).is_err(), "{field}");
    }
    for args in [
        json!({"query":"x","limit":-1}),
        json!({"query":"x","limit":1.5}),
        json!({"query":"x","cursor":{}}),
    ] {
        assert!(serde_json::from_value::<Query>(args).is_err());
    }
    let schema = definition();
    assert_eq!(schema.parameters["additionalProperties"], false);
    assert_eq!(schema.parameters["properties"]["limit"]["maximum"], 20);
}
