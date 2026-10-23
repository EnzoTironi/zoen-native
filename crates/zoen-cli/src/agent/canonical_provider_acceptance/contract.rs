//! Existing operator fixture counters and exact read/write route admission.

use super::*;

impl Budget {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            deadline: Instant::now() + WALL,
            counts: Mutex::default(),
            evidence: None,
        })
    }
    pub(super) fn with_evidence(directory: &Path) -> Arc<Self> {
        Arc::new(Self {
            deadline: Instant::now() + WALL,
            counts: Mutex::default(),
            evidence: Some(directory.join("provider-trace.json")),
        })
    }
    pub(super) fn value(state: &Counts) -> Value {
        json!({"clefDispatches":state.clef,"codexTurnsStarted":state.codex,"refusedBeforeDelegation":state.refused,
            "trace":state.trace,"limits":{"clefDispatches":CLEF_CAP,"codexTurns":CODEX_CAP,"wallSeconds":WALL.as_secs()},
            "providerInternalRetriesAndBillingUnmeasured":true})
    }
    pub(super) fn persist(&self, state: &Counts) {
        if let Some(path) = &self.evidence {
            private_write(path, &Self::value(state));
        }
    }
    pub(super) fn reserve(&self, kind: &str) -> Result<usize, String> {
        let mut state = self.counts.lock().unwrap();
        let full = match kind {
            "clef" => state.clef >= CLEF_CAP,
            "codex" => state.codex >= CODEX_CAP,
            _ => true,
        };
        if Instant::now() >= self.deadline || full {
            state.refused += 1;
            self.persist(&state);
            return Err("synthetic acceptance dispatch/time budget exhausted".into());
        }
        match kind {
            "clef" => state.clef += 1,
            "codex" => state.codex += 1,
            _ => unreachable!(),
        }
        let index = state.trace.len();
        state.trace.push(json!({"kind":kind,"state":"started"}));
        // This is proof metadata, never production authority. A crash/timeout
        // retains the reservation before a delegate can receive a request.
        self.persist(&state);
        Ok(index)
    }
    pub(super) fn note(&self, value: Value) {
        let mut state = self.counts.lock().unwrap();
        state.trace.push(value);
        self.persist(&state);
    }
    pub(super) fn snapshot(&self) -> Value {
        let state = self.counts.lock().unwrap();
        Self::value(&state)
    }
    pub(super) fn intact(&self) -> bool {
        self.counts.lock().unwrap().refused == 0 && Instant::now() < self.deadline
    }
}

impl Expected {
    pub(super) fn read(&self) -> ToolCall {
        ToolCall {
            requires_review: false,
            id: "synthetic-read".into(),
            name: crate::deckcraft::model_name("read_deckcraft", &self.item).unwrap(),
            arguments: json!({"artifactId":self.item}),
        }
    }
    pub(super) fn write(&self, read: &ToolStep) -> Result<ToolCall, String> {
        if read.is_error
            || read.call.name != self.read().name
            || read.call.arguments != self.read().arguments
        {
            return Err("acceptance guard: verified-read-required".into());
        }
        let result: Value = serde_json::from_str(&read.result)
            .map_err(|_| "acceptance guard: invalid-read-result")?;
        if result["scope"]["artifactId"] != self.item {
            return Err("acceptance guard: read-artifact".into());
        }
        if result["engineRevision"] != zoen_agentd::deckcraft::ENGINE_REVISION {
            return Err("acceptance guard: read-engine".into());
        }
        if result["head"]["version"]
            .as_str()
            .and_then(|v| v.parse::<u32>().ok())
            != Some(self.version)
        {
            return Err("acceptance guard: read-version".into());
        }
        if result["head"]["sha256"] != self.head {
            return Err("acceptance guard: read-hash".into());
        }
        let slide = result["slides"][0]["id"]
            .as_u64()
            .filter(|id| *id <= u32::MAX as u64)
            .ok_or("acceptance guard: native-slide-id")?;
        Ok(ToolCall {
            requires_review: false,
            id: "synthetic-write".into(),
            name: crate::deckcraft::model_name("deckcraft_slide_notes", &self.item).unwrap(),
            arguments: json!({"artifactId":self.item,"version":self.version,"sha256":self.head,"slideId":slide,"text":NOTES}),
        })
    }
    pub(super) fn admit(
        &self,
        worker_binding: &str,
        steps: &[ToolStep],
        output: &BrainOut,
    ) -> Result<(), String> {
        if worker_binding != self.worker_binding {
            return Err("acceptance guard: worker-binding".into());
        }
        if steps.iter().any(|s| s.is_error) {
            return Err("acceptance guard: failed-step".into());
        }
        match (steps, output) {
            ([], BrainOut::CallTools(calls))
                if calls.len() == 1
                    && calls[0].name == self.read().name
                    && calls[0].arguments == self.read().arguments =>
            {
                Ok(())
            }
            ([read], BrainOut::CallTools(calls)) => {
                let expected = self.write(read)?;
                if calls.len() != 1 {
                    return Err("acceptance guard: write-call-count".into());
                }
                if calls[0].id == read.call.id || calls[0].name != expected.name {
                    return Err("acceptance guard: write-call-identity".into());
                }
                if calls[0].arguments != expected.arguments {
                    return Err("acceptance guard: write-arguments".into());
                }
                Ok(())
            }
            ([read, write], BrainOut::Say(text)) => {
                let expected = self.write(read)?;
                if text != REPLY
                    || write.call.id == read.call.id
                    || write.call.name != expected.name
                    || write.call.arguments != expected.arguments
                {
                    return Err("acceptance guard: final-reply-or-step".into());
                }
                Ok(())
            }
            _ => Err("acceptance guard: route".into()),
        }
    }
    pub(super) fn shape(
        &self,
        worker_binding: &str,
        steps: &[ToolStep],
        output: &BrainOut,
    ) -> Value {
        let (kind, calls) = match output {
            BrainOut::CallTools(calls) => ("call", calls.as_slice()),
            BrainOut::Say(_) => ("say", &[][..]),
            BrainOut::Silence => ("quiet", &[][..]),
            BrainOut::CreateAgent { .. } => ("create-agent", &[][..]),
        };
        let read = steps
            .first()
            .and_then(|s| serde_json::from_str::<Value>(&s.result).ok());
        json!({"kind":kind,"callCount":calls.len(),"completedStepCount":steps.len(),
            "workerBindingMatches":worker_binding==self.worker_binding,
            "readFirstNativeSlideId":read.as_ref().and_then(|r|r["slides"][0]["id"].as_u64()),
            "calls":calls.iter().map(|call|json!({
                "readAliasMatches":call.name==self.read().name,
                "writeAliasMatches":call.name==crate::deckcraft::model_name("deckcraft_slide_notes",&self.item).unwrap(),
                "argumentKeys":call.arguments.as_object().map(|a|a.keys().filter(|k|["artifactId","version","sha256","slideId","text"].contains(&k.as_str())).cloned().collect::<Vec<_>>()),
                "argumentKeyCount":call.arguments.as_object().map(|a|a.len()),
                "artifactMatches":call.arguments["artifactId"]==self.item,
                "versionMatches":call.arguments["version"].as_u64()==Some(self.version as u64),
                "hashMatches":call.arguments["sha256"]==self.head,
                "nativeSlideId":call.arguments["slideId"].as_u64(),
                "textMatches":call.arguments["text"]==NOTES
            })).collect::<Vec<_>>()})
    }
}
