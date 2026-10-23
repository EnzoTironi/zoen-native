use super::*;

type Input = (Vec<ToolDef>, Vec<ToolStep>, usize);
#[derive(Default)]
struct Observed {
    tools: TestTools,
    inputs: Mutex<Vec<Input>>,
    outcomes: Mutex<Vec<(ToolCall, ToolObservation)>>,
    refuse_prepare: bool,
}
impl ToolHost for Observed {
    fn tools(&self) -> Vec<ToolDef> {
        self.tools.tools()
    }
    fn call(&self, call: &ToolCall) -> Result<String, String> {
        self.tools.call(call)
    }
    fn observe_model_input(&self, tools: &[ToolDef], steps: &[ToolStep], round: usize) {
        self.inputs
            .lock()
            .unwrap()
            .push((tools.to_vec(), steps.to_vec(), round));
    }
    fn observe_tool_outcome(&self, call: &ToolCall, outcome: ToolObservation) {
        self.outcomes.lock().unwrap().push((call.clone(), outcome));
    }
    fn prepare(&self, _: &ToolAsk) -> Result<(), String> {
        if self.refuse_prepare {
            Err("synthetic private preparation error".into())
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn observation_matches_each_actual_brain_offer_and_result_without_extra_execution() {
    let seen = Arc::new(Mutex::new(vec![]));
    let runtime = runtime(
        vec![call("read_current")],
        seen.clone(),
        Arc::new(Mutex::new(vec![])),
    );
    let host = Observed::default();
    let def = definition();
    let out = runtime
        .turn_with_tools(context(&def), &[], &inbound(), &host, vec![])
        .await
        .unwrap();
    assert!(matches!(out, TurnOutcome::Say { .. }));
    let inputs = host.inputs.lock().unwrap();
    assert_eq!(inputs.len(), 2);
    assert_eq!(
        inputs
            .iter()
            .map(|(_, steps, _)| steps.clone())
            .collect::<Vec<_>>(),
        *seen.lock().unwrap()
    );
    assert!(inputs.iter().all(|(tools, _, _)| *tools == host.tools()));
    assert_eq!((inputs[0].2, inputs[1].2), (0, 1));
    assert_eq!(inputs[1].1.len(), 1);
    assert_eq!(
        *host.tools.executed.lock().unwrap(),
        vec![call("read_current")]
    );
    assert_eq!(
        *host.outcomes.lock().unwrap(),
        vec![(call("read_current"), ToolObservation::HostOk)]
    );
}

#[tokio::test]
async fn held_review_observes_no_result_or_second_think_until_canonical_admission() {
    let mut selected = call("read_current");
    selected.requires_review = true;
    let seen = Arc::new(Mutex::new(vec![]));
    let runtime = runtime(
        vec![selected.clone()],
        seen.clone(),
        Arc::new(Mutex::new(vec![])),
    );
    let host = Observed::default();
    let def = definition();
    let TurnOutcome::ToolAsk { ask } = runtime
        .turn_with_tools(context(&def), &[], &inbound(), &host, vec![])
        .await
        .unwrap()
    else {
        panic!("held")
    };
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert_eq!(
        *host.outcomes.lock().unwrap(),
        vec![(selected.clone(), ToolObservation::Held)]
    );
    assert!(host.tools.executed.lock().unwrap().is_empty());
    let denied = TestAdmission {
        expected: ask.clone(),
        executed: Mutex::new(vec![]),
        refuse: true,
    };
    assert!(runtime
        .resume_tool_ask(context(&def), &[], &inbound(), &host, ask, &denied)
        .await
        .is_err());
    assert_eq!(seen.lock().unwrap().len(), 1);
    assert_eq!(
        host.outcomes.lock().unwrap().last(),
        Some(&(selected, ToolObservation::AdmissionError))
    );
}

#[test]
fn failed_preparation_observation_does_not_execute_retry_or_hide_the_existing_error() {
    let runtime = runtime(
        vec![],
        Arc::new(Mutex::new(vec![])),
        Arc::new(Mutex::new(vec![])),
    );
    let host = Observed {
        refuse_prepare: true,
        ..Observed::default()
    };
    let def = definition();
    assert!(matches!(
        runtime.run_tool_calls(
            context(&def),
            &inbound(),
            &host,
            &host.tools(),
            &mut vec![],
            vec![call("read_current")],
            0
        ),
        Err(HostError::ToolContinuation(_))
    ));
    assert!(host.tools.executed.lock().unwrap().is_empty());
    assert_eq!(
        *host.outcomes.lock().unwrap(),
        vec![(call("read_current"), ToolObservation::PreparationRefused)]
    );
}
