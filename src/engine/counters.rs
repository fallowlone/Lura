//! Outline numbering for `H1`–`H6` in document order (see `docs/SPEC.md`).

use std::collections::HashMap;

use super::arena::{DocumentArena, NodeId};
use super::styles::{BoxContent, BoxKind};

/// Computes `NodeId` → outline label (`1`, `1.2`, …) for each heading.
pub fn compute_heading_numbers(styled: &DocumentArena) -> HashMap<NodeId, String> {
    let mut out = HashMap::new();
    let mut counts = [0u32; 6];
    for &root in &styled.roots {
        walk(root, styled, &mut counts, &mut out);
    }
    out
}

fn walk(
    id: NodeId,
    styled: &DocumentArena,
    counts: &mut [u32; 6],
    out: &mut HashMap<NodeId, String>,
) {
    let node = styled.get(id);
    if let BoxKind::Heading(level) = node.kind {
        let l = level as usize;
        if (1..=6).contains(&l) {
            for c in counts.iter_mut().skip(l).take(6 - l) {
                *c = 0;
            }
            counts[l - 1] += 1;
            let label = format_outline_label(l, counts);
            out.insert(id, label);
        }
    }

    if let BoxContent::Children(children) = &node.content {
        for &child in children {
            walk(child, styled, counts, out);
        }
    }
}

/// Multi-level label: `1`, `1.2`, `2.1` when H3 follows H1 (skipped levels omit zero slots).
fn format_outline_label(level: usize, counts: &[u32; 6]) -> String {
    let l = level;
    let mut parts = Vec::new();
    for &n in counts.iter().take(l) {
        if n == 0 {
            continue;
        }
        parts.push(n.to_string());
    }
    if parts.is_empty() {
        return "0".to_string();
    }
    parts.join(".")
}

/// Replaces `{{sec}}` in heading text / inline runs using `compute_heading_numbers`.
pub fn apply_sec_placeholders(styled: &mut DocumentArena, numbers: &HashMap<NodeId, String>) {
    let roots: Vec<NodeId> = styled.roots.to_vec();
    for root in roots {
        apply_sec_recursive(root, styled, numbers);
    }
}

fn apply_sec_recursive(id: NodeId, styled: &mut DocumentArena, numbers: &HashMap<NodeId, String>) {
    let is_heading = matches!(styled.get(id).kind, BoxKind::Heading(_));
    let sec = numbers.get(&id).cloned();

    if is_heading && let Some(ref label) = sec {
        let node = styled.get_mut(id);
        match &mut node.content {
            BoxContent::Text(s) => {
                *s = s.replace("{{sec}}", label);
            }
            BoxContent::Inline(runs) => {
                for run in runs.iter_mut() {
                    run.text = run.text.replace("{{sec}}", label);
                }
            }
            _ => {}
        }
    }

    let children: Vec<NodeId> = match &styled.get(id).content {
        BoxContent::Children(ch) => ch.clone(),
        _ => Vec::new(),
    };
    for child in children {
        apply_sec_recursive(child, styled, numbers);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::resolver::build_styled_tree;
    use crate::lexer::Lexer;
    use crate::parser::{self, id, Parser};

    fn parse_doc(input: &str) -> crate::parser::ast::Document {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        let doc = parser.parse().expect("parse");
        let doc = parser::resolver::resolve(doc);
        id::assign_ids(doc)
    }

    #[test]
    fn outline_numbers_nested() {
        let src = "
PAGE(
  H1(First)
  H2(A)
  H2(B)
  H1(Second)
  H3(Deep)
)
";
        let doc = parse_doc(src);
        let styled = build_styled_tree(&doc);
        let m = compute_heading_numbers(&styled);
        let labels: Vec<String> = styled
            .roots
            .iter()
            .flat_map(|&r| collect_heading_labels(r, &styled, &m))
            .collect();
        assert_eq!(
            labels,
            vec!["1", "1.1", "1.2", "2", "2.1"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    fn collect_heading_labels(
        id: NodeId,
        styled: &DocumentArena,
        m: &HashMap<NodeId, String>,
    ) -> Vec<String> {
        let mut v = Vec::new();
        let node = styled.get(id);
        if let BoxKind::Heading(_) = node.kind
            && let Some(l) = m.get(&id)
        {
            v.push(l.clone());
        }
        if let BoxContent::Children(ch) = &node.content {
            for &c in ch {
                v.extend(collect_heading_labels(c, styled, m));
            }
        }
        v
    }
}
