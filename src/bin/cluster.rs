//! Fuzzy clustering tool for long-tail payees.
//!
//! Groups similar payee strings using trigram Jaccard similarity
//! with blocking keys for efficient comparison.

use anyhow::Result;
use rusqlite::Connection;
use serde_json::json;
use std::collections::{HashMap, HashSet};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let db_path = if let Some(pos) = args.iter().position(|a| a == "--db") {
        args.get(pos + 1)
            .map(|s| s.as_str())
            .unwrap_or("lab/working.db")
    } else {
        "lab/working.db"
    };

    let threshold: f64 = if let Some(pos) = args.iter().position(|a| a == "--threshold") {
        args.get(pos + 1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.8)
    } else {
        0.8
    };

    let max_count: i64 = if let Some(pos) = args.iter().position(|a| a == "--max-count") {
        args.get(pos + 1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(10)
    } else {
        10
    };

    let conn = Connection::open(db_path)?;

    // 1. Query long-tail payees
    let mut stmt =
        conn.prepare("SELECT payee, COUNT(*) as cnt FROM transactions GROUP BY payee HAVING cnt <= ?1")?;
    let payees: Vec<(String, i64)> = stmt
        .query_map([max_count], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    eprintln!("Loaded {} long-tail payees", payees.len());

    // 2. Build blocking keys → payee indices
    let mut blocks: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, (payee, _)) in payees.iter().enumerate() {
        let lower = payee.to_lowercase();
        for key in blocking_keys(&lower) {
            blocks.entry(key).or_default().push(i);
        }
    }

    // 3. Compute trigrams per payee
    let trigrams: Vec<HashSet<[u8; 3]>> = payees
        .iter()
        .map(|(p, _)| char_trigrams(&p.to_lowercase()))
        .collect();

    // 4. Find pairs above threshold within each block
    let mut edges: Vec<(usize, usize, f64)> = Vec::new();
    let mut seen: HashSet<(usize, usize)> = HashSet::new();

    for members in blocks.values() {
        if members.len() > 500 {
            continue; // skip oversized blocks
        }
        for (ai, &a) in members.iter().enumerate() {
            for &b in &members[ai + 1..] {
                let pair = if a < b { (a, b) } else { (b, a) };
                if !seen.insert(pair) {
                    continue;
                }
                let sim = jaccard(&trigrams[a], &trigrams[b]);
                if sim >= threshold {
                    edges.push((a, b, sim));
                }
            }
        }
    }

    eprintln!("Found {} edges above threshold {}", edges.len(), threshold);

    // 5. Union-find to form clusters
    let mut parent: Vec<usize> = (0..payees.len()).collect();
    for &(a, b, _) in &edges {
        union(&mut parent, a, b);
    }

    // Group by root
    let mut cluster_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..payees.len() {
        let root = find(&mut parent, i);
        cluster_map.entry(root).or_default().push(i);
    }

    // Build edge lookup for similarity info
    let mut edge_map: HashMap<(usize, usize), f64> = HashMap::new();
    for &(a, b, sim) in &edges {
        edge_map.insert((a, b), sim);
    }

    // 6. Output clusters with >1 member
    let mut clusters: Vec<serde_json::Value> = Vec::new();
    for members in cluster_map.values() {
        if members.len() < 2 {
            continue;
        }
        let mut max_sim: f64 = 0.0;
        let mut min_sim: f64 = 1.0;
        for (ai, &a) in members.iter().enumerate() {
            for &b in &members[ai + 1..] {
                let pair = if a < b { (a, b) } else { (b, a) };
                if let Some(&sim) = edge_map.get(&pair) {
                    max_sim = max_sim.max(sim);
                    min_sim = min_sim.min(sim);
                }
            }
        }

        // Suggested canonical: member with highest count
        let canonical_idx = *members
            .iter()
            .max_by_key(|&&i| payees[i].1)
            .unwrap();

        let member_json: Vec<serde_json::Value> = members
            .iter()
            .map(|&i| {
                json!({
                    "payee": payees[i].0,
                    "count": payees[i].1,
                })
            })
            .collect();

        clusters.push(json!({
            "members": member_json,
            "max_sim": (max_sim * 1000.0).round() / 1000.0,
            "min_sim": (min_sim * 1000.0).round() / 1000.0,
            "suggested_canonical": payees[canonical_idx].0,
        }));
    }

    // Sort by cluster size descending
    clusters.sort_by(|a, b| {
        let a_len = a["members"].as_array().map_or(0, |v| v.len());
        let b_len = b["members"].as_array().map_or(0, |v| v.len());
        b_len.cmp(&a_len)
    });

    eprintln!("Found {} clusters", clusters.len());
    println!("{}", serde_json::to_string_pretty(&clusters)?);

    Ok(())
}

/// Generate blocking keys: first 6 alpha chars, first 6 consonants (both lowercased).
fn blocking_keys(s: &str) -> Vec<String> {
    let mut keys = Vec::new();

    let alpha: String = s.chars().filter(|c| c.is_ascii_alphabetic()).take(6).collect();
    if alpha.len() >= 3 {
        keys.push(format!("a:{}", alpha));
    }

    let consonants: String = s
        .chars()
        .filter(|c| c.is_ascii_alphabetic() && !"aeiou".contains(*c))
        .take(6)
        .collect();
    if consonants.len() >= 3 {
        keys.push(format!("c:{}", consonants));
    }

    keys
}

/// Compute character trigrams from a string.
fn char_trigrams(s: &str) -> HashSet<[u8; 3]> {
    let bytes: Vec<u8> = s.bytes().collect();
    let mut set = HashSet::new();
    if bytes.len() >= 3 {
        for w in bytes.windows(3) {
            set.insert([w[0], w[1], w[2]]);
        }
    }
    set
}

/// Jaccard similarity between two trigram sets.
fn jaccard(a: &HashSet<[u8; 3]>, b: &HashSet<[u8; 3]>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.len() + b.len() - intersection;
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

/// Union-find: find with path compression.
fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// Union-find: union by index (simple).
fn union(parent: &mut [usize], a: usize, b: usize) {
    let ra = find(parent, a);
    let rb = find(parent, b);
    if ra != rb {
        parent[rb] = ra;
    }
}
