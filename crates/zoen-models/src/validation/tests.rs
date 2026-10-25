use super::{
    prepare, prepare_block, prepare_message, CompletionRequest, GatewayError, Message, RigTool,
    ToolName,
};
use crate::{InputMessage, ModelRequest, OutputBlock};
use serde_json::json;

#[test]
fn validated_model_mapping_keeps_order_payload_and_typed_names() {
    let arguments = json!({"q":"Olá 🌥", "nested":[1,true]});
    let mut request: ModelRequest = serde_json::from_value(json!({
        "context":{"attempt_id":"a","run_id":"r","owner":"owner","agent":"agent","device":"d","space":"s","authority_version":"a1","definition_version":"d1","price_version":"p1"},
        "operation":"chat_completion",
        "messages":[{"role":"system","text":"instructions"},{"role":"user","text":"Olá 🌥"}],
        "tools":[{"name":"lookup","description":"Find","parameters":{"type":"object"}}],
        "max_output_tokens":20
    })).unwrap();
    request.messages.push(InputMessage::Assistant {
        blocks: vec![OutputBlock::ToolCall {
            call_id: "call-1".into(),
            name: "lookup".into(),
            arguments: arguments.clone(),
        }],
    });
    request.messages.push(InputMessage::ToolResult {
        call_id: "call-1".into(),
        name: "lookup".into(),
        text: "Result".into(),
    });
    let actual = prepare(&request, 20, 100_000).unwrap();
    let expected = CompletionRequest::new(Message::tool_result(
        rig_core::message::CallId::from_wire("call-1"),
        ToolName::new("lookup").unwrap(),
        "Result",
    ))
    .messages(vec![
        Message::system("instructions"),
        Message::user("Olá 🌥"),
        Message::Assistant(rig_core::message::AssistantMessage::new(vec![
            rig_core::message::AssistantContent::tool_call(
                "call-1",
                ToolName::new("lookup").unwrap(),
                arguments,
            ),
        ])),
    ])
    .tools(vec![RigTool::new(
        ToolName::new("lookup").unwrap(),
        "Find",
        json!({"type":"object"}),
    )])
    .max_tokens(20);
    assert_eq!(
        serde_json::to_vec(&actual).unwrap(),
        serde_json::to_vec(&expected).unwrap()
    );
    request.messages.clear();
    assert!(matches!(
        prepare(&request, 20, 100_000),
        Err(GatewayError::InvalidRequest)
    ));
}

#[test]
fn rig_name_refusal_propagates_even_if_mapping_is_called_directly() {
    let block = OutputBlock::ToolCall {
        call_id: "call-1".into(),
        name: String::new(),
        arguments: json!({}),
    };
    assert!(matches!(
        prepare_block(&block),
        Err(GatewayError::InvalidRequest)
    ));
    let message = InputMessage::Assistant {
        blocks: vec![block],
    };
    assert!(matches!(
        prepare_message(&message),
        Err(GatewayError::InvalidRequest)
    ));
    let result = InputMessage::ToolResult {
        call_id: "call-1".into(),
        name: String::new(),
        text: "result".into(),
    };
    assert!(matches!(
        prepare_message(&result),
        Err(GatewayError::InvalidRequest)
    ));
}
