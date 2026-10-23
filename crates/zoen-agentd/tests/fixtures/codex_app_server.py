#!/usr/bin/python3
import sys,json,time,os
with open(os.path.splitext(os.path.abspath(sys.argv[0]))[0]+'.json')as f:config=json.load(f)
MODE=config['mode']
MARKER=config['marker']
EXPECTED=config['expected']
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
  with open(MARKER+'.schema','w')as f:json.dump(m['params']['outputSchema'],f)
  if MODE=='oversize':print('x'*(1024*1024+1),flush=True);time.sleep(10)
  if MODE=='hang':time.sleep(10)
  if MODE=='request':send({'id':99,'method':'item/commandExecution/requestApproval','params':{'command':'must-not-run'}});time.sleep(10)
  if MODE=='overtake':send({'method':'item/started','params':{'threadId':'thread-fixture','item':{'type':'userMessage'}}})
  send({'id':i,'result':{'turn':{'id':'turn-fixture'}}})
  tool='unknown'if MODE=='unknown-tool'else'edit_deck'
  text=json.dumps({'outcome':{'kind':'review'if MODE=='review'else'call','calls':[{'id':'call1','name':tool,'arguments_json':json.dumps({'shapeId':42,'text':'Real proposal'})}]}})
  if MODE=='nested-contract':
   context=json.loads(m['params']['input'][0]['text'])
   offered=context['offered_tools']
   text=json.dumps({'outcome':{'kind':'call','calls':[{'id':'nested1','name':offered[0]['name'],'arguments_json':'{}'}]}}if offered else{'outcome':{'kind':'quiet'}})
  if MODE=='after-tools':
   context=json.loads(m['params']['input'][0]['text'])
   assert context['completed_tool_steps']==EXPECTED['steps']
   step=context['completed_tool_steps'][0]
   assert step['is_error']is False
   read=json.loads(step['result'])
   assert read==EXPECTED['read']and read['slides']and read['slides'][0]['shapes']
   assert read['scope']==EXPECTED['scope']and read['head']==EXPECTED['head']
   arguments={'shapeId':read['slides'][0]['shapes'][0]['id'],'text':'Synthetic next step'}
   text=json.dumps({'outcome':{'kind':'call','calls':[{'id':'after-read','name':'edit_deck','arguments_json':json.dumps(arguments)}]}})
  send({'method':'item/completed','params':{'threadId':'thread-fixture','turnId':'turn-fixture','item':{'type':'agentMessage','text':text}}})
  send({'method':'turn/completed','params':{'threadId':'thread-fixture','turn':{'id':'turn-fixture','status':MODE if MODE in ['failed','interrupted']else'completed'}}})
  continue
 else:raise RuntimeError('Unexpected RPC method')
 send({'id':i,'result':result})
