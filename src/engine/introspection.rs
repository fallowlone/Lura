//! Page placeholders and related helpers (`docs/SPEC.md`).

use std::collections::HashMap;

use super::arena::{DocumentArena, NodeId};
use super::styles::BoxContent;

const PREFIX: &str = "{{page:";

/// `true` if any text or inline run contains `{{page:…}}`.
pub fn arena_has_page_placeholders(styled: &DocumentArena) -> bool {
    styled.roots.iter().any(|&r| node_has_page_placeholder(r, styled))
}

fn node_has_page_placeholder(id: NodeId, styled: &DocumentArena) -> bool {
    let node = styled.get(id);
    match &node.content {
        BoxContent::Text(s) => s.contains(PREFIX),
        BoxContent::Inline(runs) => runs.iter().any(|r| r.text.contains(PREFIX)),
        BoxContent::Children(ch) => ch.iter().any(|&c| node_has_page_placeholder(c, styled)),
        BoxContent::Empty => false,
    }
}

/// Substitute `{{page:BLOCK_ID}}` using 1-based page indices from pagination.
pub fn apply_page_placeholders(styled: &mut DocumentArena, page_by_id: &HashMap<String, u32>) {
    let roots: Vec<NodeId> = styled.roots.to_vec();
    for root in roots {
        rewrite_node_pages(root, styled, page_by_id);
    }
}

fn rewrite_node_pages(id: NodeId, styled: &mut DocumentArena, page_by_id: &HashMap<String, u32>) {
    let children: Vec<NodeId> = match &styled.get(id).content {
        BoxContent::Children(ch) => ch.clone(),
        _ => Vec::new(),
    };

    {
        let node = styled.get_mut(id);
        match &mut node.content {
            BoxContent::Text(s) => {
                *s = replace_page_placeholders(s, page_by_id);
            }
            BoxContent::Inline(runs) => {
                for run in runs.iter_mut() {
                    run.text = replace_page_placeholders(&run.text, page_by_id);
                }
            }
            _ => {}
        }
    }

    for c in children {
        rewrite_node_pages(c, styled, page_by_id);
    }
}

fn replace_page_placeholders(s: &str, page_by_id: &HashMap<String, u32>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find(PREFIX) {
        out.push_str(&rest[..start]);
        rest = &rest[start + PREFIX.len()..];
        if let Some(end) = rest.find("}}") {
            let id = rest[..end].trim();
            rest = &rest[end + 2..];
            let page = page_by_id.get(id).copied().unwrap_or(0);
            if page == 0 {
                out.push('?');
            } else {
                out.push_str(&page.to_string());
            }
        } else {
            out.push_str(PREFIX);
            break;
        }
    }
    out.push_str(rest);
    out
}

/// Stable fingerprint for convergence of the block→page map between passes.
pub fn fingerprint_page_map(map: &HashMap<String, u32>) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut pairs: Vec<_> = map.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    let mut h = DefaultHasher::new();
    for (k, v) in pairs {
        k.hash(&mut h);
        v.hash(&mut h);
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_page_known_and_unknown() {
        let mut m = HashMap::new();
        m.insert("a".to_string(), 3u32);
        assert_eq!(
            replace_page_placeholders("See {{page:a}} and {{page:missing}}.", &m),
            "See 3 and ?."
        );
    }
}
