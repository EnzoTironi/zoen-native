use super::super::profile::{AgentKind, Definition, ModelTier};
use super::*;
use std::os::unix::fs::PermissionsExt;

const CHILD: &str = r#"#!/usr/bin/python3
import sys,json,time,os
MODE=__MODE__
MARKER=__MARKER__
EXPECTED=json.loads(__EXPECTED__)
if '--version' in sys.argv:
 print('codex-cli '+('0.162.0' if MODE=='version' else '0.163.0'));sys.exit(0)
settings=[sys.argv[i+1]for i,a in enumerate(sys.argv[:-1])if a=='-c']
flags={s.split('=',1)[0].replace('features.',''):False for s in settings if s.startswith('features.')and s.endswith('=false')}
assert 'features.shell_tool=false'in settings and 'features.unified_exec=false'in settings
assert 'features.apps=false'in settings and 'features.plugins=false'in settings
assert not any(k in os.environ for k in ['OPENAI_API_KEY','CLOUDFLARE_WORKERS_AI_TOKEN'])
with open(MARKER+'.pid','w')as f:f.write(str(os.getpid()))
for raw in sys.stdin:
 m=json.loads(raw);method=m.get('method');i=m.get('id')
 def send(v):print(json.dumps(v),flush=True)
 if method=='initialized':continue
 if method=='initialize':result={'userAgent':'fixture/0.163.0'}
 elif method=='config/read':
  denied=any(s.startswith('mcp_servers={')and'enabled=false'in s for s in settings)
  result={'config':{'features':flags,'web_search':'disabled','project_doc_max_bytes':0,'mcp_servers':{'existing':{'enabled':False if denied and MODE!='mcp'else True}},'notify':['forbidden-callback']if MODE=='notify'else[]}}
 elif method=='account/read':result={'account':{'type':'apiKey'if MODE=='api-key'else'chatgpt'}}
 elif method=='model/list':result={'data':[{'id':'fixture-model','isDefault':True}],'nextCursor':None}
 elif method=='thread/start':
  p=m['params'];assert p['environments']==[]and p['dynamicTools']==[]and p['ephemeral']is True
  assert p['approvalPolicy']=='never'and p['runtimeWorkspaceRoots']==[]
  assert 'review proposes the same tools'in p['developerInstructions']
  assert 'say is an answer or clarification and cannot create a canonical approval request'in p['developerInstructions']
  assert 'without a successful tool result'in p['developerInstructions']
  result={'thread':{'id':'thread-fixture','ephemeral':True},'modelProvider':'openai','model':'fixture-model','sandbox':{'type':'readOnly','networkAccess':False},'instructionSources':[]}
 elif method=='turn/start':
  assert m['params']['environments']==[]
  with open(MARKER,'a')as f:f.write('turn\n')
  if MODE=='oversize':print('x'*(1024*1024+1),flush=True);time.sleep(10)
  if MODE=='hang':time.sleep(10)
  if MODE=='request':send({'id':99,'method':'item/commandExecution/requestApproval','params':{'command':'must-not-run'}});time.sleep(10)
  if MODE=='overtake':send({'method':'item/started','params':{'threadId':'thread-fixture','item':{'type':'userMessage'}}})
  send({'id':i,'result':{'turn':{'id':'turn-fixture'}}})
  tool='unknown'if MODE=='unknown-tool'else'edit_deck'
  text=json.dumps({'kind':'review'if MODE=='review'else'call','text':'','calls':[{'id':'call1','name':tool,'arguments_json':json.dumps({'shapeId':42,'text':'Real proposal'})}]})
  if MODE=='after-tools':
   context=json.loads(m['params']['input'][0]['text'])
   assert context['completed_tool_steps']==EXPECTED['steps']
   step=context['completed_tool_steps'][0]
   assert step['is_error']is False
   read=json.loads(step['result'])
   assert read==EXPECTED['read']and read['slides']and read['slides'][0]['shapes']
   assert read['scope']==EXPECTED['scope']and read['head']==EXPECTED['head']
   arguments={'shapeId':read['slides'][0]['shapes'][0]['id'],'text':'Synthetic next step'}
   text=json.dumps({'kind':'call','text':'','calls':[{'id':'after-read','name':'edit_deck','arguments_json':json.dumps(arguments)}]})
  send({'method':'item/completed','params':{'threadId':'thread-fixture','turnId':'turn-fixture','item':{'type':'agentMessage','text':text}}})
  send({'method':'turn/completed','params':{'threadId':'thread-fixture','turn':{'id':'turn-fixture','status':MODE if MODE in ['failed','interrupted']else'completed'}}})
  continue
 else:raise RuntimeError('Unexpected RPC method')
 send({'id':i,'result':result})
"#;

fn fixture(mode: &str) -> (tempfile::TempDir, CodexBrain, PathBuf) {
    fixture_with_expected(mode, Value::Null)
}
fn fixture_with_expected(mode: &str, expected: Value) -> (tempfile::TempDir, CodexBrain, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fake-codex");
    let marker = dir.path().join("turns");
    let source = CHILD
        .replace("__MODE__", &serde_json::to_string(mode).unwrap())
        .replace(
            "__MARKER__",
            &serde_json::to_string(marker.to_str().unwrap()).unwrap(),
        )
        .replace(
            "__EXPECTED__",
            &serde_json::to_string(&expected.to_string()).unwrap(),
        );
    std::fs::write(&path, source).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    (
        dir,
        CodexBrain {
            binary: path,
            model: None,
            timeout: Duration::from_secs(5),
        },
        marker,
    )
}
fn definition() -> Definition {
    Definition {
        version: 1,
        persona: "Propose presentation edits only".into(),
        rules: vec![],
        state: json!({}),
        kind: AgentKind::User,
        model_tier: ModelTier::Cheap,
    }
}
fn tool() -> ToolDef {
    ToolDef {
        name: "edit_deck".into(),
        description: "Edit one offered deck".into(),
        parameters: json!({"type":"object","properties":{"shapeId":{"type":"integer"},"text":{"type":"string"}},"required":["shapeId","text"]}),
    }
}
async fn think(brain: &CodexBrain) -> Result<BrainOut, String> {
    let def = definition();
    let cx = TurnContext {
        agent: "agent",
        device: "device",
        owner: "owner",
        definition: &def,
    };
    brain
        .think_with_tools(
            cx,
            &[],
            &Turn {
                space: "space".into(),
                from: "owner".into(),
                text: "Edit offered shape".into(),
            },
            &[tool()],
            &[],
        )
        .await
}
#[tokio::test]
async fn isolated_metadata_admission_returns_only_exact_offered_proposal() {
    let (_dir, brain, marker) = fixture("success");
    let BrainOut::CallTools(calls) = think(&brain).await.unwrap() else {
        panic!("expected proposal")
    };
    assert_eq!(
        calls,
        vec![ToolCall {
            requires_review: false,
            id: "call1".into(),
            name: "edit_deck".into(),
            arguments: json!({"shapeId":42,"text":"Real proposal"})
        }]
    );
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "turn\n");
}
#[tokio::test]
async fn api_key_incompatible_version_and_still_enabled_mcp_refuse_before_inference() {
    for mode in ["api-key", "version", "mcp", "notify"] {
        let (_dir, brain, marker) = fixture(mode);
        assert!(think(&brain).await.is_err(), "{mode}");
        assert!(!marker.exists(), "no inference for {mode}");
    }
}
#[tokio::test]
async fn unknown_tools_server_requests_and_incomplete_turns_never_escape_as_proposals() {
    for mode in ["unknown-tool", "request", "failed", "interrupted"] {
        let (_dir, brain, marker) = fixture(mode);
        assert!(think(&brain).await.is_err(), "{mode}");
        assert!(marker.exists());
    }
}
#[tokio::test]
async fn output_bound_is_enforced_before_a_missing_newline_allocates_unbounded_memory() {
    let (_dir, brain, _) = fixture("oversize");
    assert_eq!(
        think(&brain).await.unwrap_err(),
        "Codex output exceeds limit"
    );
}
#[tokio::test]
async fn timeout_retires_an_actual_stalled_child_process() {
    let (_dir, mut brain, marker) = fixture("hang");
    brain.timeout = Duration::from_millis(500);
    assert_eq!(
        think(&brain).await.unwrap_err(),
        "Codex local inference timed out"
    );
    let pid = std::fs::read_to_string(marker.with_extension("pid")).unwrap();
    let until = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let alive = std::process::Command::new("/bin/kill")
            .args(["-0", &pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success();
        if !alive {
            break;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "stalled child remained alive"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
#[test]
fn malformed_ambiguous_unoffered_and_duplicate_calls_are_refused() {
    for output in [
        json!({"kind":"call","text":"speak and call","calls":[]}),
        json!({"kind":"call","text":"","calls":[{"id":"same","name":"edit_deck","arguments_json":"{}"},{"id":"same","name":"edit_deck","arguments_json":"{}"}]}),
        json!({"kind":"call","text":"","calls":[{"id":"one","name":"edit_deck","arguments_json":"[]"}]}),
        json!({"kind":"say","text":"ok","calls":[],"extra":true}),
    ] {
        assert!(parse_output(&output.to_string(), &[tool()]).is_err());
    }
    assert_eq!(
        parse_output(r#"{"kind":"quiet","text":"","calls":[]}"#, &[]).unwrap(),
        BrainOut::Silence
    );
}

#[tokio::test]
async fn notifications_overtaking_turn_admission_are_not_dropped_or_misclassified() {
    let (_dir, brain, _) = fixture("overtake");
    assert!(matches!(think(&brain).await.unwrap(),BrainOut::CallTools(calls)if calls.len()==1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "explicit existing operator-signed WASI pins; fake Codex only, no provider login"]
async fn actual_wasi_read_metadata_reaches_fake_codex_completed_tool_steps_exactly() {
    use crate::deckcraft::{DeckcraftWorker, ENGINE_REVISION};
    use crate::SignedManifest;
    use ed25519_dalek::VerifyingKey;
    use sha2::{Digest, Sha256};

    // Public deployment pins only. No fixture signing key or fallback component.
    let component = std::env::var("ZOEN_DECKCRAFT_WORKER_COMPONENT").unwrap();
    let manifest = std::env::var("ZOEN_DECKCRAFT_WORKER_MANIFEST").unwrap();
    let publisher = std::env::var("ZOEN_DECKCRAFT_WORKER_PUBLISHER").unwrap();
    let key: [u8; 32] = hex::decode(publisher).unwrap().try_into().unwrap();
    let signed: SignedManifest = serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
    let worker = DeckcraftWorker::load(
        vec![VerifyingKey::from_bytes(&key).unwrap()],
        &signed,
        &std::fs::read(component).unwrap(),
    )
    .unwrap();
    let bytes = include_bytes!("../../../../miniapps/resources/deckcraft.empty.deckcraft");
    let scope = json!({"artifactId":"it_deck_codex_read_fixture","actorId":"synthetic_agent","actorKind":"agent"});
    let head = json!({"version":"1","sha256":hex::encode(Sha256::digest(bytes))});
    let reply = worker
        .run(
            &json!({"schema":1,"open":{"scope":scope,"head":head},"command":null}),
            bytes,
        )
        .await
        .unwrap();
    assert!(reply.bytes.is_empty());
    assert_eq!(reply.metadata["scope"], scope);
    assert_eq!(reply.metadata["head"], head);
    assert_eq!(reply.metadata["engineRevision"], ENGINE_REVISION);
    assert_eq!(reply.metadata["slides"].as_array().unwrap().len(), 1);
    let native_shape = reply.metadata["slides"][0]["shapes"][0]["id"]
        .as_u64()
        .unwrap();
    let step = ToolStep {
        call: ToolCall {
            requires_review: false,
            id: "accepted-synthetic-read".into(),
            name: "read_deck".into(),
            arguments: json!({"artifactId":"it_deck_codex_read_fixture"}),
        },
        result: reply.metadata.to_string(),
        is_error: false,
    };
    let expected = json!({"steps":[step],"read":reply.metadata,"scope":scope,"head":head});
    let (_dir, brain, marker) = fixture_with_expected("after-tools", expected.clone());
    let def = definition();
    let cx = TurnContext {
        agent: "synthetic_agent",
        device: "synthetic_device",
        owner: "synthetic_owner",
        definition: &def,
    };
    let inbound = Turn {
        space: "synthetic_space".into(),
        from: "synthetic_owner".into(),
        text: "Use the existing read context for the next proposal.".into(),
    };
    let out = brain
        .think_with_tools(cx, &[], &inbound, &[tool()], std::slice::from_ref(&step))
        .await
        .unwrap();
    assert_eq!(
        out,
        BrainOut::CallTools(vec![ToolCall {
            requires_review: false,
            id: "after-read".into(),
            name: "edit_deck".into(),
            arguments: json!({"shapeId":native_shape,"text":"Synthetic next step"}),
        }])
    );
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "turn\n");

    // The same executable assertion must reject a receipt-only replacement;
    // otherwise a green test would not detect the payload-loss hypothesis.
    let (_dir, brain, _) = fixture_with_expected("after-tools", expected);
    let mut receipt_only = step;
    receipt_only.result =
        json!({"scope":scope,"head":head,"engineRevision":ENGINE_REVISION}).to_string();
    assert!(brain
        .think_with_tools(cx, &[], &inbound, &[tool()], &[receipt_only])
        .await
        .is_err());
}

#[path = "tests/explicit_review.rs"]
mod explicit_review;
