#!/usr/bin/env python3
"""Generate bounded Unicode data from the exact pinned Telegram emoji1.txt.
The first-section/group-skip rule follows StaticEmojiMapping (GPL-2.0-or-later).
No locale/user/account/network state is read. Generated data uses selected GPLv3.
"""
from pathlib import Path
import hashlib,json,re,sys
source=Path(sys.argv[1]); target=Path(sys.argv[2])
raw=source.read_bytes()
assert hashlib.sha256(raw).hexdigest()=="6fecb9b7edbd7ddfb18dc757e1f3de6ce2aaa89f1bdab0251caf28169cd3e58c"
parts=raw.decode('utf-8').replace('\r\n','\n').split('=========================================')
assert len(parts)==4
sections=parts[0].strip().split('\n\n\n')
keys=['people','animalsAndNature','foodAndDrink','activityAndSport','travelAndPlaces','objects','symbols','flags']
assert len(sections)==len(keys)
output=[];variants={}
for key,section in zip(keys,sections):
    items=section.strip().replace('"','').split(',');values=[];index=0
    while index<len(items):
        item=items[index];value=item.strip();index+=1
        if not value:continue
        values.append(value)
        if item.startswith('\n'):
            while index<len(items) and not items[index].startswith('\n'):index+=1
    assert values and len(values)==len(set(values))
    output.append((key,values))
    for line in section.splitlines():
        group=re.findall(r'"([^\"]+)"',line)
        if len(group)>1 and any(any(0x1f3fb<=ord(c)<=0x1f3ff for c in value) for value in group[1:]):
            assert group[0] in values
            variants[group[0]]=list(dict.fromkeys(group[1:]))
assert [len(v) for _,v in output]==[411,214,130,84,131,119,302,260]
assert sum(len(v) for _,v in output)==1651
q=lambda value:json.dumps(value,ensure_ascii=False)
lines=['// Generated from Telegram-iOS f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838 emoji1.txt.',
'// Derived data: Telegram-iOS contributors, GPL-2.0-or-later; GPLv3 selected.',
'// See EmojiKeyboard/NOTICE.md and EmojiKeyboard/generate-telegram-emoji.py. Do not hand edit.',
'import Foundation','','enum ZoenEmojiKeyboardData {',
'  struct Section { let key: String; let emoji: [String] }',
'  static let sections: [Section] = [']
for key,values in output:
    lines.append('    Section(key: '+q(key)+', emoji: [')
    for i in range(0,len(values),12):lines.append('      '+', '.join(q(x) for x in values[i:i+12])+',')
    lines.append('    ]),')
lines+=['  ]','  static let variants: [String: [String]] = [']
for base,values in variants.items():lines.append('    '+q(base)+': ['+', '.join(q(v) for v in values)+'],')
lines+=['  ]','  static let allowed = Set(sections.flatMap(\.emoji) + variants.values.flatMap { $0 })','}']
text='\n'.join(lines)+'\n';target.parent.mkdir(parents=True,exist_ok=True);target.write_text(text)
print(json.dumps({'categories':len(output),'baseEmoji':sum(len(v) for _,v in output),'variantGroups':len(variants),'allowed':len(set(v for _,g in output for v in g)|set(v for g in variants.values() for v in g)),'sourceSha256':hashlib.sha256(raw).hexdigest(),'generatedSha256':hashlib.sha256(text.encode()).hexdigest()},indent=2))
