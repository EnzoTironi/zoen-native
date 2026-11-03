use super::*;

fn rectangle() -> Value {
    json!({"id":"shape_1","type":"rectangle","x":1,"y":2,"width":40,"height":30})
}
fn scene(elements: &[Value]) -> Value {
    json!({"type":"excalidraw","version":2,"elements":elements,"appState":{},"files":{}})
}
fn accepted(value: &Value) -> bool {
    Scene::parse(&value.to_string()).is_ok()
}

#[test]
fn compact_metadata_ignores_deleted_elements_and_keeps_passive_extensions() {
    let mut value = scene(&[rectangle()]);
    let mut text = rectangle();
    text["id"] = json!("text_1");
    text["type"] = json!("text");
    text["text"] = json!("Private drawing text");
    let mut deleted = rectangle();
    deleted["id"] = json!("deleted_1");
    deleted["isDeleted"] = json!(true);
    value["elements"]
        .as_array_mut()
        .unwrap()
        .extend([text, deleted]);
    value["futurePassive"] = json!({"retained":[1,"unknown value",{"nested":true}]});
    let input = format!(" {value}\n");
    let parsed = Scene::parse(&input).unwrap();
    assert_eq!(parsed.metadata(), r#"{"elementCount":2,"textCount":1}"#);
    assert_eq!(
        Scene::parse(EMPTY).unwrap().metadata(),
        r#"{"elementCount":0,"textCount":0}"#
    );
}

#[test]
fn element_shape_and_cross_references_are_checked() {
    let base = scene(&[rectangle()]);
    assert!(accepted(&base));
    for (field, invalid) in [
        ("id", json!("invalid id")),
        ("type", json!("embeddable")),
        ("x", json!(1_000_000_001)),
        ("width", json!(-1)),
        ("height", json!(null)),
        ("text", json!("x".repeat(65_537))),
        ("isDeleted", json!(1)),
        ("points", json!([[1, 2, 3]])),
        ("points", json!([[1, "2"]])),
        ("containerId", json!("missing")),
        ("frameId", json!("missing")),
        ("startBinding", json!({"elementId":"missing"})),
        ("endBinding", json!({"elementId":"missing"})),
        ("boundElements", json!([{"id":"missing"}])),
    ] {
        let mut invalid_scene = base.clone();
        invalid_scene["elements"][0][field] = invalid;
        assert!(!accepted(&invalid_scene), "{field}");
    }
    let mut duplicate = base.clone();
    duplicate["elements"]
        .as_array_mut()
        .unwrap()
        .push(rectangle());
    assert!(!accepted(&duplicate));
    let mut valid_refs = base;
    let mut second = rectangle();
    second["id"] = json!("shape_2");
    second["containerId"] = json!("shape_1");
    second["startBinding"] = json!({"elementId":"shape_1"});
    second["boundElements"] = json!([{"id":"shape_1"}]);
    valid_refs["elements"].as_array_mut().unwrap().push(second);
    assert!(accepted(&valid_refs));
}

#[test]
fn embedded_raster_signatures_match_the_declared_mime_without_remote_fetches() {
    for (mime, bytes) in [
        ("image/png", b"\x89PNG\r\n\x1a\nsynthetic".as_slice()),
        ("image/jpeg", b"\xff\xd8\xffsynthetic"),
        ("image/webp", b"RIFF0000WEBPsynthetic"),
        ("image/gif", b"GIF89asynthetic"),
    ] {
        let mut image = rectangle();
        image["type"] = json!("image");
        image["fileId"] = json!("image_1");
        let mut value = scene(&[image]);
        let payload = base64::engine::general_purpose::STANDARD.encode(bytes);
        value["files"] = json!({"image_1":{"id":"image_1","mimeType":mime,"dataURL":format!("data:{mime};base64,{payload}")}});
        assert!(accepted(&value), "{mime}");
        for bad_url in [
            "https://example.com/private.png",
            "data:image/svg+xml;base64,PHN2Zz4=",
            "data:image/png;base64,bm90IGEgcG5n",
            "javascript:alert(1)",
        ] {
            let mut invalid = value.clone();
            invalid["files"]["image_1"]["dataURL"] = json!(bad_url);
            assert!(!accepted(&invalid));
        }
        value["files"]["image_1"]["id"] = json!("different");
        assert!(!accepted(&value));
    }
    let mut orphan = rectangle();
    orphan["type"] = json!("image");
    orphan["fileId"] = json!("missing");
    assert!(!accepted(&scene(&[orphan])));
}

#[test]
fn passive_json_is_bounded_before_any_host_storage_effect() {
    assert!(Scene::parse("").is_err());
    assert!(Scene::parse(&" ".repeat(MAX_SCENE_BYTES + 1)).is_err());
    for bad in [
        json!(9_007_199_254_740_992_u64),
        json!("null\0byte"),
        json!({"key\0":true}),
    ] {
        let mut value = scene(&[]);
        value["futurePassive"] = bad;
        assert!(!accepted(&value));
    }
    let mut nested = json!(null);
    for _ in 0..65 {
        nested = json!([nested]);
    }
    let mut value = scene(&[]);
    value["futurePassive"] = nested;
    assert!(!accepted(&value));
    value["futurePassive"] = json!(vec![0; 500_001]);
    assert!(!accepted(&value));
    value = scene(
        &(0..10_001)
            .map(|n| {
                let mut shape = rectangle();
                shape["id"] = json!(format!("shape_{n}"));
                shape
            })
            .collect::<Vec<_>>(),
    );
    assert!(!accepted(&value));
    value = scene(&[]);
    value["version"] = json!(3);
    assert!(!accepted(&value));
    value["version"] = json!(2);
    value["appState"] = json!([]);
    assert!(!accepted(&value));
}
