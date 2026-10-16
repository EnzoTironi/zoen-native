//! Deterministic hackathon reducer. Adapters supply authenticated actors and event-time
//! parent roles; MLS creation/membership must succeed before publishing team mutations.
use crate::Role;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub space: String,
    pub members: Vec<String>,
    pub project_url: Option<String>,
    pub files: Vec<String>,
    pub demo_slot_ms: Option<i64>,
    pub votes: usize,
    pub score: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScheduleEvent { pub title: String, pub at_ms: i64 }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TeamRequest { pub team_id: String, pub identity_id: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hackathon {
    pub id: String,
    pub title: String,
    pub deadline_ms: i64,
    pub registered: BTreeSet<String>,
    pub profiles: BTreeMap<String, String>,
    pub teams: Vec<Team>,
    pub schedule: Vec<ScheduleEvent>,
    pub judges: BTreeSet<String>,
    pub winners: Vec<String>,
    pub finalized: bool,
    pub pending_team_requests: Vec<TeamRequest>,
    ballots: BTreeMap<String, String>,
    scores: BTreeMap<String, BTreeMap<String, u64>>,
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.trim().is_empty() && s.len() <= 4096).ok_or_else(|| format!("Invalid {key}"))
}
fn admin(role: Role) -> bool { matches!(role, Role::Owner | Role::Admin) }
fn require(ok: bool, reason: &str) -> Result<(), String> { if ok { Ok(()) } else { Err(reason.into()) } }

impl Hackathon {
    pub fn create(actor: &str, role: Role, now: i64, v: &Value) -> Result<Self, String> {
        require(admin(role), "Administrators only")?;
        let deadline = v["deadline_ms"].as_i64().ok_or("Invalid deadline")?;
        require(deadline > now, "Deadline must be in the future")?;
        Ok(Self { id: text(v,"id")?.into(), title: text(v,"title")?.into(), deadline_ms: deadline,
            registered: BTreeSet::new(), profiles: BTreeMap::new(), teams: vec![], schedule: vec![],
            judges: BTreeSet::from([actor.into()]), winners: vec![], finalized: false,
            pending_team_requests: vec![], ballots: BTreeMap::new(), scores: BTreeMap::new() })
    }
    pub fn apply(&mut self, actor: &str, role: Role, now: i64, v: &Value) -> Result<(), String> {
        require(role != Role::Reader, "Readers cannot participate")?;
        require(!self.finalized, "Hackathon is finalized")?;
        let action = text(v,"action")?;
        match action {
            "register" => {
                require(now < self.deadline_ms, "Registration closed")?;
                self.registered.insert(actor.into());
                self.profiles.insert(actor.into(), v["skills"].as_str().unwrap_or("").chars().take(512).collect());
            }
            "team_create" => {
                require(now < self.deadline_ms && self.registered.contains(actor), "Register before forming a team")?;
                require(!self.teams.iter().any(|t| t.members.iter().any(|m| m == actor)), "Already on a team")?;
                let id = text(v,"id")?;
                require(!self.teams.iter().any(|t| t.id == id), "Duplicate team")?;
                self.teams.push(Team { id:id.into(), name:text(v,"name")?.into(), space:text(v,"space")?.into(), members:vec![actor.into()], project_url:None, files:vec![], demo_slot_ms:None, votes:0, score:0 });
            }
            "team_join" => {
                require(now < self.deadline_ms, "Team formation closed")?;
                let team_id = text(v,"team_id")?;
                let who = v["identity_id"].as_str().unwrap_or(actor);
                require(self.registered.contains(who), "Applicant must register")?;
                require(!self.teams.iter().any(|t| t.members.iter().any(|m| m == who)), "Already on a team")?;
                let team = self.teams.iter_mut().find(|t| t.id == team_id).ok_or("Team not found")?;
                if who == actor {
                    require(!self.pending_team_requests.iter().any(|r| r.team_id == team_id && r.identity_id == actor), "Request already pending")?;
                    self.pending_team_requests.push(TeamRequest { team_id:team_id.into(), identity_id:actor.into() });
                } else {
                    require(team.members.first().is_some_and(|m| m == actor) || admin(role), "Team leader or administrator approval required")?;
                    require(self.pending_team_requests.iter().any(|r| r.team_id == team_id && r.identity_id == who), "No pending team request")?;
                    team.members.push(who.into());
                    self.pending_team_requests.retain(|r| r.identity_id != who);
                }
            }
            "project_submit" => {
                require(now < self.deadline_ms, "Submissions are frozen")?;
                let team_id = text(v,"team_id")?;
                let slot = self.deadline_ms.checked_add((self.teams.iter().filter(|t| t.demo_slot_ms.is_some()).count() as i64) * 600_000).ok_or("Invalid demo slot")?;
                let team = self.teams.iter_mut().find(|t| t.id == team_id).ok_or("Team not found")?;
                require(team.members.iter().any(|m| m == actor), "Only team members submit")?;
                let url = v["url"].as_str().unwrap_or("");
                require(url.is_empty() || (url.starts_with("https://") && url.len() <= 4096), "Submission link must use HTTPS")?;
                let files = v["file_ids"].as_array().ok_or("Invalid files")?.iter().map(|v| v.as_str().map(str::to_owned).ok_or("Invalid file".to_string())).collect::<Result<Vec<_>,_>>()?;
                require(!url.is_empty() || !files.is_empty(), "Submission needs a link or file")?;
                require(files.len() <= 32, "Too many files")?;
                team.project_url = if url.is_empty() { None } else { Some(url.into()) };
                team.files = files;
                if team.demo_slot_ms.is_none() { team.demo_slot_ms = Some(slot); }
            }
            "vote" => {
                require(now >= self.deadline_ms && self.registered.contains(actor), "Voting opens after submissions close for registered participants")?;
                require(!self.ballots.contains_key(actor), "One vote per participant")?;
                let team_id = text(v,"team_id")?;
                let team = self.teams.iter_mut().find(|t| t.id == team_id).ok_or("Team not found")?;
                require(team.demo_slot_ms.is_some() && !team.members.iter().any(|m| m == actor), "Cannot vote for own or unsubmitted team")?;
                team.votes += 1;
                self.ballots.insert(actor.into(), team_id.into());
            }
            "judge" => {
                require(now >= self.deadline_ms && (admin(role) || self.judges.contains(actor)), "Assigned judges only after submission deadline")?;
                let team_id = text(v,"team_id")?;
                let score = v["score"].as_u64().filter(|s| *s <= 100).ok_or("Score must be 0–100")?;
                let team = self.teams.iter_mut().find(|t| t.id == team_id).ok_or("Team not found")?;
                require(team.demo_slot_ms.is_some() && !team.members.iter().any(|m| m == actor), "Cannot judge own or unsubmitted team")?;
                self.scores.entry(team_id.into()).or_default().insert(actor.into(), score);
                team.score = self.scores[team_id].values().sum();
            }
            "set_judge" => { require(admin(role), "Administrators only")?; self.judges.insert(text(v,"identity_id")?.into()); }
            "schedule_event" => {
                require(admin(role), "Administrators only")?;
                let at_ms = v["at_ms"].as_i64().filter(|n| *n >= 0).ok_or("Invalid schedule time")?;
                self.schedule.push(ScheduleEvent { title:text(v,"title")?.into(), at_ms });
                self.schedule.sort_by_key(|e| e.at_ms);
            }
            "finalize" => {
                require(admin(role) && now >= self.deadline_ms, "Administrators finalize after deadline")?;
                let mut eligible: Vec<_> = self.teams.iter().filter(|t| t.demo_slot_ms.is_some()).collect();
                require(!eligible.is_empty(), "No submitted projects")?;
                eligible.sort_by(|a,b| (b.score + b.votes as u64).cmp(&(a.score+a.votes as u64)).then_with(|| a.id.cmp(&b.id)));
                self.winners = vec![eligible[0].id.clone()];
                self.finalized = true;
            }
            _ => return Err("Unknown hackathon action".into()),
        }
        Ok(())
    }
    pub fn projection(&self, actor: &str, role: Role) -> Value {
        let mut value = serde_json::to_value(self).expect("hackathon serializes");
        let object = value.as_object_mut().expect("object");
        object.remove("ballots"); object.remove("scores");
        object.insert("can_judge".into(), json!(admin(role) || self.judges.contains(actor)));
        object.insert("pending_team_requests".into(), json!(self.pending_team_requests.iter().filter(|r| admin(role) || self.teams.iter().any(|t| t.id == r.team_id && t.members.first().is_some_and(|m| m == actor))).collect::<Vec<_>>()));
        if let Some(teams) = object.get_mut("teams").and_then(Value::as_array_mut) {
            for team in teams { if !team["members"].as_array().is_some_and(|m| m.iter().any(|m| m == actor)) { team["space"] = Value::Null; } }
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hack() -> Hackathon { Hackathon::create("owner", Role::Owner, 1, &json!({"id":"h","title":"Build","deadline_ms":100})).unwrap() }
    fn register(h: &mut Hackathon, who: &str) { h.apply(who,Role::Member,2,&json!({"action":"register"})).unwrap(); }
    fn team(h: &mut Hackathon, who: &str, id: &str) { register(h,who); h.apply(who,Role::Member,3,&json!({"action":"team_create","id":id,"name":id,"space":format!("encrypted-{id}")})).unwrap(); h.apply(who,Role::Member,4,&json!({"action":"project_submit","team_id":id,"url":"https://example.com/demo","file_ids":[]})).unwrap(); }
    #[test] fn hackathon_authority_deadline_and_frozen_slot() {
        assert!(Hackathon::create("m",Role::Member,1,&json!({"id":"h","title":"bad","deadline_ms":10})).is_err());
        let mut h=hack(); team(&mut h,"alice","a");
        let slot=h.teams[0].demo_slot_ms;
        h.apply("alice",Role::Member,5,&json!({"action":"project_submit","team_id":"a","url":"https://example.com/new","file_ids":[]})).unwrap();
        assert_eq!(slot,h.teams[0].demo_slot_ms);
        assert!(h.apply("alice",Role::Member,100,&json!({"action":"project_submit","team_id":"a","url":"https://example.com/late","file_ids":[]})).is_err());
        assert!(h.apply("alice",Role::Member,101,&json!({"action":"judge","team_id":"a","score":100})).is_err());
    }
    #[test] fn hackathon_votes_judges_and_exactly_once_finalization() {
        let mut h=hack(); team(&mut h,"alice","a"); team(&mut h,"bob","b"); register(&mut h,"voter");
        assert!(h.apply("alice",Role::Member,101,&json!({"action":"vote","team_id":"a"})).is_err());
        assert!(h.apply("stranger",Role::Member,101,&json!({"action":"vote","team_id":"a"})).is_err());
        h.apply("voter",Role::Member,101,&json!({"action":"vote","team_id":"a"})).unwrap();
        assert!(h.apply("voter",Role::Member,102,&json!({"action":"vote","team_id":"b"})).is_err());
        h.apply("owner",Role::Owner,102,&json!({"action":"judge","team_id":"b","score":5})).unwrap();
        h.apply("owner",Role::Owner,103,&json!({"action":"finalize"})).unwrap();
        assert_eq!(h.winners,vec!["b"]);
        assert!(h.apply("owner",Role::Owner,104,&json!({"action":"finalize"})).is_err());
    }
    #[test] fn hackathon_team_requests_need_leader_approval() {
        let mut h=hack(); register(&mut h,"alice"); register(&mut h,"bob"); register(&mut h,"mallory");
        h.apply("alice",Role::Member,3,&json!({"action":"team_create","id":"a","name":"A","space":"mls-a"})).unwrap();
        h.apply("bob",Role::Member,4,&json!({"action":"team_join","team_id":"a"})).unwrap();
        assert_eq!(h.teams[0].members,vec!["alice"]);
        assert!(h.apply("mallory",Role::Member,5,&json!({"action":"team_join","team_id":"a","identity_id":"bob"})).is_err());
        h.apply("alice",Role::Member,5,&json!({"action":"team_join","team_id":"a","identity_id":"bob"})).unwrap();
        assert_eq!(h.teams[0].members,vec!["alice","bob"]);
        assert!(h.projection("mallory",Role::Member)["teams"][0]["space"].is_null());
    }
}
