use super::super::profile::{AgentKind, Definition, ModelTier};
use super::*;
use std::os::unix::fs::PermissionsExt;

const CHILD: &str = r#"#!/usr/bin/python3
import sys,json,time,os
MODE=__MODE__
MARKER=__MARKER__
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
  text=json.dumps({'kind':'call','text':'','calls':[{'id':'call1','name':tool,'arguments_json':json.dumps({'shapeId':42,'text':'Real proposal'})}]})
  send({'method':'item/completed','params':{'threadId':'thread-fixture','turnId':'turn-fixture','item':{'type':'agentMessage','text':text}}})
  send({'method':'turn/completed','params':{'threadId':'thread-fixture','turn':{'id':'turn-fixture','status':MODE if MODE in ['failed','interrupted']else'completed'}}})
  continue
 else:raise RuntimeError('Unexpected RPC method')
 send({'id':i,'result':result})
"#;

fn fixture(mode: &str) -> (tempfile::TempDir, CodexBrain, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fake-codex");
    let marker = dir.path().join("turns");
    let source = CHILD
        .replace("__MODE__", &serde_json::to_string(mode).unwrap())
        .replace(
            "__MARKER__",
            &serde_json::to_string(marker.to_str().unwrap()).unwrap(),
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
