//! Private bounded SQL evidence. Failures expose names/counts/digests, never rows.
use super::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct Table {
    rows: Vec<String>,
    digest: String,
}
pub(super) struct Snapshot {
    tables: BTreeMap<String, Table>,
}
impl Snapshot {
    pub(super) async fn capture(f: &Fixture) -> Self {
        let mut transaction = f.world.runtime.finance.pool.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *transaction)
            .await
            .unwrap();
        let names: Vec<String> = sqlx::query_scalar(
            "SELECT tablename::TEXT FROM pg_tables WHERE schemaname='public' ORDER BY tablename LIMIT 129",
        ).fetch_all(&mut *transaction).await.unwrap();
        assert!(
            names.len() <= 128,
            "SQL fixture exceeds bounded table catalog"
        );
        let mut tables = BTreeMap::new();
        let mut row_count = 0usize;
        let mut bytes = 0usize;
        for name in names {
            let quoted = name.replace('"', "\"\"");
            let rows: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT row_to_json(r)::TEXT FROM public.\"{quoted}\" r ORDER BY row_to_json(r)::TEXT LIMIT 4097",
            ))).fetch_all(&mut *transaction).await.unwrap();
            row_count = row_count.checked_add(rows.len()).unwrap();
            assert!(row_count <= 4096, "SQL fixture exceeds bounded total rows");
            for row in &rows {
                bytes = bytes.checked_add(row.len()).unwrap();
                assert!(
                    bytes < 5_000_000,
                    "SQL fixture exceeds bounded private evidence bytes"
                );
            }
            let digest = hash(&serde_json::to_vec(&rows).unwrap());
            tables.insert(name, Table { rows, digest });
        }
        transaction.commit().await.unwrap();
        Self { tables }
    }

    fn same(&self, after: &Self) -> bool {
        self.tables.len() == after.tables.len()
            && self.tables.iter().all(|(name, table)| {
                after
                    .tables
                    .get(name)
                    .is_some_and(|other| table.rows == other.rows)
            })
    }
    pub(super) fn assert_exact(&self, after: &Self) {
        assert!(
            self.same(after),
            "SQL changed across logical FDB restore: {}",
            self.summary(after)
        );
    }
    pub(super) fn assert_only_authenticated_ws_touch(&self, after: &Self, actor: &Author) {
        assert!(
            self.only_authenticated_ws_touch(after, actor),
            "unexpected SQL effects across registered WS publication: {}",
            self.summary(after)
        );
        println!("managed publication SQL: only exact authenticated devices.last_seen advanced; every row/other column/table including custody checkpoint unchanged");
    }
    fn only_authenticated_ws_touch(&self, after: &Self, actor: &Author) -> bool {
        if self.tables.keys().ne(after.tables.keys()) {
            return false;
        }
        for (name, before) in &self.tables {
            let Some(after) = after.tables.get(name) else {
                return false;
            };
            if name != "devices" && before.rows != after.rows {
                return false;
            }
        }
        let Some(before) = self.tables.get("devices").and_then(device_rows) else {
            return false;
        };
        let Some(after) = after.tables.get("devices").and_then(device_rows) else {
            return false;
        };
        if before.keys().ne(after.keys()) {
            return false;
        }
        let mut touched = 0usize;
        for (device, original) in before {
            let Some(current) = after.get(&device) else {
                return false;
            };
            if original == *current {
                continue;
            }
            if actor.device.as_deref() != Some(device.as_str())
                || original.get("identity").and_then(serde_json::Value::as_str)
                    != Some(actor.identity.as_str())
                || original.get("cert").and_then(serde_json::Value::as_str) != actor.cert.as_deref()
                || original.get("revoked_at") != Some(&serde_json::Value::Null)
            {
                return false;
            }
            let (Some(previous_time), Some(current_time)) = (
                original
                    .get("last_seen")
                    .and_then(serde_json::Value::as_str),
                current.get("last_seen").and_then(serde_json::Value::as_str),
            ) else {
                return false;
            };
            let (Ok(previous_time), Ok(current_time)) = (
                chrono::DateTime::parse_from_rfc3339(previous_time),
                chrono::DateTime::parse_from_rfc3339(current_time),
            ) else {
                return false;
            };
            if current_time <= previous_time {
                return false;
            }
            let mut immutable_before = original;
            let mut immutable_after = current.clone();
            immutable_before.remove("last_seen");
            immutable_after.remove("last_seen");
            if immutable_before != immutable_after {
                return false;
            }
            touched += 1;
        }
        touched == 1
    }

    fn summary(&self, after: &Self) -> String {
        let names: BTreeSet<_> = self.tables.keys().chain(after.tables.keys()).collect();
        let changes: Vec<_> = names.into_iter().filter_map(|name| {
            let before = self.tables.get(name); let next = after.tables.get(name);
            if before.map(|table|&table.rows) == next.map(|table|&table.rows) { return None; }
            let before_rows = before.map(|table|table.rows.as_slice()).unwrap_or_default();
            let after_rows = next.map(|table|table.rows.as_slice()).unwrap_or_default();
            Some(json!({"table":name,"before_count":before_rows.len(),"after_count":after_rows.len(),
                "before_sha256":before.map(|table|table.digest.as_str()),
                "after_sha256":next.map(|table|table.digest.as_str()),
                "changed_columns":changed_columns(before_rows, after_rows)}))
        }).collect();
        serde_json::to_string(&changes).unwrap()
    }
}
fn device_rows(
    table: &Table,
) -> Option<BTreeMap<String, serde_json::Map<String, serde_json::Value>>> {
    let mut result = BTreeMap::new();
    for row in &table.rows {
        let row: serde_json::Value = serde_json::from_str(row).ok()?;
        let row = row.as_object()?.clone();
        let device = row.get("device")?.as_str()?.to_string();
        if result.insert(device, row).is_some() {
            return None;
        }
    }
    Some(result)
}
fn changed_columns(before: &[String], after: &[String]) -> Vec<String> {
    fn columns(rows: &[String]) -> BTreeMap<String, Vec<String>> {
        let mut columns = BTreeMap::<String, Vec<String>>::new();
        for row in rows {
            let row: serde_json::Value = serde_json::from_str(row).unwrap();
            for (name, value) in row.as_object().unwrap() {
                columns
                    .entry(name.clone())
                    .or_default()
                    .push(serde_json::to_string(value).unwrap());
            }
        }
        for values in columns.values_mut() {
            values.sort();
        }
        columns
    }
    let before = columns(before);
    let after = columns(after);
    let names: BTreeSet<_> = before.keys().chain(after.keys()).cloned().collect();
    names
        .into_iter()
        .filter(|name| before.get(name) != after.get(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(actor: &Author, time: &str) -> Snapshot {
        let device = json!({"identity":actor.identity,"device":actor.device,"cert":actor.cert,
            "linked_at":"2026-10-23T10:00:00+00:00","last_seen":time,"revoked_at":null});
        let mut tables = BTreeMap::new();
        for (name, rows) in [
            ("devices", vec![device.to_string()]),
            ("runtime_image_checkpoint", vec!["{\"sequence\":1}".into()]),
            ("runtime_postings", Vec::new()),
        ] {
            tables.insert(
                name.into(),
                Table {
                    digest: hash(&serde_json::to_vec(&rows).unwrap()),
                    rows,
                },
            );
        }
        Snapshot { tables }
    }
    #[test]
    fn only_exact_authenticated_timestamp_advance_is_allowed() {
        let actor = Author::device(&Signer::generate(), Signer::generate());
        let before = snapshot(&actor, "2026-10-23T10:01:00+00:00");
        let after = snapshot(&actor, "2026-10-23T10:02:00+00:00");
        assert!(before.only_authenticated_ws_touch(&after, &actor));
        assert!(
            !before.same(&after),
            "full restore equality still includes timestamp"
        );
        for field in ["identity", "device", "cert", "linked_at", "revoked_at"] {
            let mut changed = snapshot(&actor, "2026-10-23T10:02:00+00:00");
            let table = changed.tables.get_mut("devices").unwrap();
            let mut row: serde_json::Value = serde_json::from_str(&table.rows[0]).unwrap();
            row[field] = json!("changed");
            table.rows[0] = row.to_string();
            assert!(
                !before.only_authenticated_ws_touch(&changed, &actor),
                "{field}"
            );
        }
        let backwards = snapshot(&actor, "2026-10-23T10:00:00+00:00");
        assert!(!before.only_authenticated_ws_touch(&backwards, &actor));
        for table in ["runtime_image_checkpoint", "runtime_postings"] {
            let mut changed = snapshot(&actor, "2026-10-23T10:02:00+00:00");
            changed
                .tables
                .get_mut(table)
                .unwrap()
                .rows
                .push("{\"changed\":true}".into());
            assert!(
                !before.only_authenticated_ws_touch(&changed, &actor),
                "{table}"
            );
        }
        let foreign = Author::device(&Signer::generate(), Signer::generate());
        assert!(!before.only_authenticated_ws_touch(&after, &foreign));
    }
    #[test]
    fn diagnostic_contains_only_names_counts_and_hashes() {
        let actor = Author::device(&Signer::generate(), Signer::generate());
        let before = snapshot(&actor, "2026-10-23T10:01:00+00:00");
        let after = snapshot(&actor, "2026-10-23T10:02:00+00:00");
        let summary = before.summary(&after);
        let decoded: serde_json::Value = serde_json::from_str(&summary).unwrap();
        assert_eq!(decoded[0]["changed_columns"], json!(["last_seen"]));
        assert!(!summary.contains(&actor.identity));
        assert!(!summary.contains(actor.cert.as_deref().unwrap()));
        assert!(!summary.contains("2026-10-23"));
    }
}
