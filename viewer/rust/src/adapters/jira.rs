//! Jira Cloud adapter.
//!
//! [`map_issues`] (always compiled, no network) turns raw issues from
//! `GET /rest/api/3/search/jql` into a Clio [`Document`]. The HTTP client lives
//! behind the `jira` cargo feature.

use std::collections::HashSet;

use anyhow::{Context, Result};
use serde_json::Value;

use super::Loader;
use crate::model::{Document, FORMAT_VERSION, Item, Kind, Link, Status};

/// Fields requested from Jira; everything [`map_issues`] reads.
pub const FIELDS: &str = "summary,status,issuetype,parent,assignee,issuelinks,description";

pub struct JiraLoader {
    pub base_url: String,
    pub email: String,
    pub token: String,
    pub jql: String,
}

impl JiraLoader {
    /// Reads `JIRA_URL`, `JIRA_EMAIL` and `JIRA_API_TOKEN` from the environment.
    pub fn from_env(jql: impl Into<String>) -> Result<Self> {
        let get = |k: &str| std::env::var(k).with_context(|| format!("{k} is not set"));
        Ok(Self {
            base_url: get("JIRA_URL")?,
            email: get("JIRA_EMAIL")?,
            token: get("JIRA_API_TOKEN")?,
            jql: jql.into(),
        })
    }
}

impl Loader for JiraLoader {
    fn load(&self) -> Result<Document> {
        #[cfg(feature = "jira")]
        {
            map_issues(&fetch_issues(self)?)
        }
        #[cfg(not(feature = "jira"))]
        {
            anyhow::bail!(
                "this build has no Jira support; rebuild with `cargo build --features jira`"
            )
        }
    }

    fn describe(&self) -> String {
        format!("jira: {}", self.jql)
    }
}

#[cfg(feature = "jira")]
fn fetch_issues(cfg: &JiraLoader) -> Result<Vec<Value>> {
    use std::time::Duration;

    let scheme_ok = cfg.base_url.get(..8).is_some_and(|p| p.eq_ignore_ascii_case("https://"));
    if !scheme_ok {
        anyhow::bail!("JIRA_URL must start with https://");
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        // Never follow redirects: reqwest keeps Authorization on a same-host
        // https -> http downgrade.
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let endpoint = reqwest::Url::parse(&format!(
        "{}/rest/api/3/search/jql",
        cfg.base_url.trim_end_matches('/')
    ))
    .with_context(|| format!("bad JIRA_URL {:?}", cfg.base_url))?;
    let mut issues = Vec::new();
    let mut page_token: Option<String> = None;
    for _ in 0..1000 {
        let mut params = vec![
            ("jql", cfg.jql.clone()),
            ("fields", FIELDS.to_string()),
            ("maxResults", "100".to_string()),
        ];
        if let Some(t) = &page_token {
            params.push(("nextPageToken", t.clone()));
        }
        let mut url = endpoint.clone();
        url.query_pairs_mut().extend_pairs(&params);
        let resp = client
            .get(url)
            .basic_auth(&cfg.email, Some(&cfg.token))
            .header("Accept", "application/json")
            .send()
            .context("calling Jira")?;
        if resp.status().is_redirection() {
            anyhow::bail!(
                "Jira answered {} (redirect); not following it with your credentials",
                resp.status()
            );
        }
        let page: Value = resp
            .error_for_status()
            .context("Jira returned an error")?
            .json()
            .context("decoding Jira response")?;
        if let Some(arr) = page["issues"].as_array() {
            issues.extend(arr.iter().cloned());
        }
        match (page["isLast"].as_bool(), page["nextPageToken"].as_str()) {
            (Some(true), _) | (_, None) => return Ok(issues),
            (_, Some(t)) => page_token = Some(t.to_string()),
        }
    }
    anyhow::bail!("Jira pagination did not terminate after 1000 pages")
}

/// Maps raw Jira issues to a Clio document.
///
/// Parents and links that point outside the fetched set are dropped, so the
/// result always passes [`crate::model::Graph::from_document`] validation.
pub fn map_issues(issues: &[Value]) -> Result<Document> {
    let keys: HashSet<&str> = issues.iter().filter_map(|i| i["key"].as_str()).collect();
    let mut items = Vec::with_capacity(issues.len());
    let mut links = Vec::new();
    let mut seen = HashSet::new();

    for issue in issues {
        let key = issue["key"].as_str().context("Jira issue without a key")?;
        let f = &issue["fields"];
        let description = adf_text(&f["description"]);
        items.push(Item {
            id: key.to_string(),
            kind: map_kind(&f["issuetype"]),
            title: f["summary"].as_str().unwrap_or_default().to_string(),
            status: map_status(&f["status"]),
            parent: f["parent"]["key"]
                .as_str()
                .filter(|p| keys.contains(p))
                .map(str::to_string),
            assignee: f["assignee"]["displayName"].as_str().map(str::to_string),
            description: (!description.is_empty()).then_some(description),
        });

        for l in f["issuelinks"].as_array().into_iter().flatten() {
            let (from, to) = if let Some(out) = l["outwardIssue"]["key"].as_str() {
                (key, out)
            } else if let Some(inw) = l["inwardIssue"]["key"].as_str() {
                (inw, key)
            } else {
                continue;
            };
            if !keys.contains(from) || !keys.contains(to) {
                continue;
            }
            let ty = &l["type"];
            let name = snake_case(
                ty["outward"]
                    .as_str()
                    .or(ty["name"].as_str())
                    .unwrap_or("relates to"),
            );
            if seen.insert((from.to_string(), to.to_string(), name.clone())) {
                links.push(Link {
                    from: from.to_string(),
                    to: to.to_string(),
                    kind: name,
                });
            }
        }
    }

    Ok(Document {
        version: FORMAT_VERSION,
        items,
        links,
    })
}

fn map_kind(issuetype: &Value) -> Kind {
    let name = issuetype["name"].as_str().unwrap_or_default().to_lowercase();
    match name.as_str() {
        "milestone" | "initiative" => return Kind::Milestone,
        "epic" => return Kind::Epic,
        _ if name.contains("sub") => return Kind::Subtask,
        _ => {}
    }
    match issuetype["hierarchyLevel"].as_i64() {
        Some(l) if l < 0 => Kind::Subtask,
        Some(1) => Kind::Epic,
        Some(l) if l >= 2 => Kind::Milestone,
        _ => Kind::Story,
    }
}

fn map_status(status: &Value) -> Status {
    let name = status["name"].as_str().unwrap_or_default().to_lowercase();
    if name.contains("block") {
        return Status::Blocked;
    }
    match status["statusCategory"]["key"].as_str() {
        Some("done") => Status::Done,
        Some("indeterminate") => Status::InProgress,
        _ => Status::Todo,
    }
}

/// "is blocked by" -> "is_blocked_by"
fn snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.trim().chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

/// Flattens an Atlassian Document Format value (or a plain string) to text.
fn adf_text(v: &Value) -> String {
    fn walk(v: &Value, out: &mut String) {
        if let Some(t) = v["text"].as_str() {
            out.push_str(t);
        }
        if let Some(content) = v["content"].as_array() {
            for c in content {
                walk(c, out);
            }
            if matches!(v["type"].as_str(), Some("paragraph" | "heading")) {
                out.push('\n');
            }
        }
    }
    if let Some(s) = v.as_str() {
        return s.trim().to_string();
    }
    let mut out = String::new();
    walk(v, &mut out);
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_normalizes_link_names() {
        assert_eq!(snake_case("relates to"), "relates_to");
        assert_eq!(snake_case("Is Blocked By"), "is_blocked_by");
        assert_eq!(snake_case("  clones -> "), "clones");
    }
}
