//! The eviction-policy items of design decision 2, located with `syn`.
//!
//! `eviction_policy_unchanged` freezes a SHA-256 of each item's tokens.
//! `eviction_policy_diff_allowed` compares each item with the oracle commit:
//! no line may be removed, and every added line must sit inside a statement
//! that calls the resident ledger.

use crate::sha256_hex;
use proc_macro2::Span;
use quote::ToTokens;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use syn::{
    Block, Expr, ExprCall, File, ImplItem, Item, Member, Stmt,
    spanned::Spanned,
    visit::{self, Visit},
};

/// The commit whose `apps/core` is the oracle build: the harness and the
/// expected digests were recorded from it.
pub const ORACLE_COMMIT: &str = "139133a4221e11ba00b7cc93d06f1af7709415ef";

/// Types whose methods change the resident ledger. A field of one of these
/// types, or a constructor of one, marks a statement as a ledger call.
pub const LEDGER_TYPES: &[&str] = &[
    "Tally",
    "MapCharge",
    "BroadcastRing",
    "Lease",
    "ResidentLedger",
];

const STORE: &str = "apps/core/src/store.rs";
const TENANT_LOADER: &str = "apps/core/src/infrastructure/persistence/tenant_loader.rs";
const MEMORY: &str = "apps/core/src/infrastructure/observability/memory.rs";
const HYDRATION: &str = "apps/core/src/infrastructure/persistence/hydration_admission.rs";
const EVENT: &str = "apps/core/src/domain/entities/event.rs";
const READ_MODEL: &str = "apps/core/src/store_read_model.rs";

/// Every item of design decision 2's table, by file and name. A name that
/// occurs twice in one file (the two `cfg` variants of `available_bytes`) is
/// keyed by its order in the file.
pub const ITEMS: &[(&str, &str)] = &[
    (STORE, "admit_hydration"),
    (STORE, "enforce_cache_budget"),
    (STORE, "enforce_cache_budget_now"),
    (STORE, "evict_lru_until"),
    (STORE, "has_memory_headroom"),
    (STORE, "try_evict_tenant"),
    (STORE, "parse_cache_byte_budget"),
    (TENANT_LOADER, "over_budget"),
    (TENANT_LOADER, "pick_lru_excluding"),
    (TENANT_LOADER, "touch"),
    (TENANT_LOADER, "mark_loaded_with_integrity"),
    (TENANT_LOADER, "mark_unloaded"),
    (TENANT_LOADER, "set_byte_budget"),
    (TENANT_LOADER, "add_bytes"),
    (TENANT_LOADER, "total_bytes"),
    (MEMORY, "available_bytes"),
    (MEMORY, "has_headroom"),
    (HYDRATION, "DEFAULT_MEMORY_RESERVE_BYTES"),
    (HYDRATION, "reserve_from_env"),
    (EVENT, "estimated_size_bytes"),
    (READ_MODEL, "evict_model"),
    (READ_MODEL, "bound_cache"),
];

/// One located item: its key, its tokens, and the 1-based lines it spans.
pub struct Located<'a> {
    pub key: String,
    pub tokens: String,
    pub first_line: usize,
    pub last_line: usize,
    pub block: Option<&'a Block>,
}

/// Every item of `ITEMS` in `file`, in source order. An item the file does not
/// contain is an error, so a rename cannot make a digest silently disappear.
pub fn locate<'a>(file: &str, ast: &'a File) -> Result<Vec<Located<'a>>, String> {
    let names: Vec<&str> = ITEMS
        .iter()
        .filter(|(f, _)| *f == file)
        .map(|(_, name)| *name)
        .collect();
    locate_names(file, &names, ast)
}

/// `locate` for an explicit list of names. Used by the synthetic cases of the
/// diff tests, which carry one item rather than the whole file.
pub fn locate_names<'a>(
    file: &str,
    names: &[&str],
    ast: &'a File,
) -> Result<Vec<Located<'a>>, String> {
    let mut found = Vec::new();
    for name in names {
        let mut hits = Vec::new();
        collect_named(&ast.items, name, &mut hits);
        if hits.is_empty() {
            return Err(format!("{file}: item {name} not found"));
        }
        let many = hits.len() > 1;
        for (ordinal, mut hit) in hits.into_iter().enumerate() {
            if many {
                hit.key = format!("{name}#{}", ordinal + 1);
            }
            found.push(hit);
        }
    }
    found.sort_by_key(|hit| hit.first_line);
    Ok(found)
}

fn collect_named<'a>(items: &'a [Item], name: &str, out: &mut Vec<Located<'a>>) {
    for item in items {
        match item {
            Item::Mod(module) if !is_cfg_test(&module.attrs) => {
                if let Some((_, inner)) = &module.content {
                    collect_named(inner, name, out);
                }
            }
            Item::Fn(f) if f.sig.ident == name => out.push(located(
                name,
                f.span(),
                f.to_token_stream().to_string(),
                Some(f.block.as_ref()),
            )),
            Item::Const(c) if c.ident == name => out.push(located(
                name,
                c.span(),
                c.to_token_stream().to_string(),
                None,
            )),
            Item::Impl(imp) if !is_cfg_test(&imp.attrs) => {
                for inner in &imp.items {
                    match inner {
                        ImplItem::Fn(m) if m.sig.ident == name => out.push(located(
                            name,
                            m.span(),
                            m.to_token_stream().to_string(),
                            Some(&m.block),
                        )),
                        ImplItem::Const(c) if c.ident == name => out.push(located(
                            name,
                            c.span(),
                            c.to_token_stream().to_string(),
                            None,
                        )),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn located<'a>(name: &str, span: Span, tokens: String, block: Option<&'a Block>) -> Located<'a> {
    Located {
        key: name.to_string(),
        tokens,
        first_line: span.start().line,
        last_line: span.end().line,
        block,
    }
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.to_token_stream()
            .to_string()
            .replace(' ', "")
            .contains("cfg(test)")
    })
}

/// Lowercase SHA-256 of an item's tokens. Comments and layout are not tokens,
/// so they do not move the digest; any token change does.
pub fn item_digest(tokens: &str) -> String {
    sha256_hex(tokens.as_bytes())
}

/// The frozen listing: `file::key digest`, one line per item, in `ITEMS` order.
pub fn listing_from_items(items: &[(String, String)]) -> String {
    items
        .iter()
        .map(|(key, tokens)| format!("{key} {}\n", item_digest(tokens)))
        .collect()
}

/// Every item of `ITEMS` under `root`, as `(file::key, tokens)` in `ITEMS`
/// order.
pub fn collect_items(root: &Path) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let mut files: Vec<&str> = Vec::new();
    for (file, _) in ITEMS {
        if !files.contains(file) {
            files.push(file);
        }
    }
    for file in files {
        let source =
            std::fs::read_to_string(root.join(file)).map_err(|_| format!("{file} unreadable"))?;
        let ast = syn::parse_file(&source).map_err(|_| format!("{file} does not parse"))?;
        for hit in locate(file, &ast)? {
            out.push((format!("{file}::{}", hit.key), hit.tokens));
        }
    }
    Ok(out)
}

/// The frozen digest listing of the tree under `root`.
pub fn listing(root: &Path) -> Result<String, String> {
    collect_items(root).map(|items| listing_from_items(&items))
}

/// Field names declared in `ast` with a ledger type. Calls on these fields are
/// ledger calls.
pub fn ledger_fields(ast: &File) -> BTreeSet<String> {
    let mut fields = BTreeSet::new();
    collect_ledger_fields(&ast.items, &mut fields);
    fields
}

fn collect_ledger_fields(items: &[Item], fields: &mut BTreeSet<String>) {
    for item in items {
        match item {
            Item::Mod(module) => {
                if let Some((_, inner)) = &module.content {
                    collect_ledger_fields(inner, fields);
                }
            }
            Item::Struct(s) => {
                for field in &s.fields {
                    if type_is_ledger(&field.ty) {
                        if let Some(name) = &field.ident {
                            fields.insert(name.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn type_is_ledger(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .is_some_and(|s| LEDGER_TYPES.contains(&s.ident.to_string().as_str())),
        _ => false,
    }
}

/// `true` when `expr` is a method call on a ledger field, or a call through a
/// ledger type's path (`Lease::new`, `Tally::new`) or `ledger::resident()`.
fn is_ledger_call(fields: &BTreeSet<String>, expr: &Expr) -> bool {
    match expr {
        Expr::MethodCall(m) => receiver_name(&m.receiver).is_some_and(|n| fields.contains(&n)),
        Expr::Call(c) => call_is_ledger(c),
        _ => false,
    }
}

fn receiver_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Field(f) => match &f.member {
            Member::Named(id) => Some(id.to_string()),
            _ => None,
        },
        Expr::Path(p) => p.path.get_ident().map(|id| id.to_string()),
        _ => None,
    }
}

fn call_is_ledger(call: &ExprCall) -> bool {
    let Expr::Path(p) = &*call.func else {
        return false;
    };
    let names: Vec<String> = p
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    names.iter().any(|n| LEDGER_TYPES.contains(&n.as_str()))
        || (names.iter().any(|n| n == "ledger") && names.last().is_some_and(|n| n == "resident"))
}

struct HasLedgerCall<'f> {
    fields: &'f BTreeSet<String>,
    found: bool,
}

impl<'ast> Visit<'ast> for HasLedgerCall<'_> {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        if is_ledger_call(self.fields, expr) {
            self.found = true;
        }
        visit::visit_expr(self, expr);
    }
}

/// The innermost statements that contain a ledger call, as inclusive line
/// ranges. A statement whose nested statement already holds the call is not
/// listed, so a new `if` or `else` header around a ledger call is not allowed.
struct LedgerStatements<'f> {
    fields: &'f BTreeSet<String>,
    ranges: Vec<(usize, usize)>,
}

impl<'ast> Visit<'ast> for LedgerStatements<'_> {
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        let before = self.ranges.len();
        visit::visit_stmt(self, stmt);
        if self.ranges.len() == before {
            let mut probe = HasLedgerCall {
                fields: self.fields,
                found: false,
            };
            visit::visit_stmt(&mut probe, stmt);
            if probe.found {
                let span = stmt.span();
                self.ranges.push((span.start().line, span.end().line));
            }
        }
    }
}

/// Line-level check of one item against its oracle version. Returns one
/// message per violation; an empty list means the change is allowed.
pub fn diff_violations(
    file: &str,
    old_source: &str,
    new_source: &str,
) -> Result<Vec<String>, String> {
    let names: Vec<&str> = ITEMS
        .iter()
        .filter(|(f, _)| *f == file)
        .map(|(_, name)| *name)
        .collect();
    diff_items(file, &names, old_source, new_source)
}

/// `diff_violations` for the named items of `file` only.
pub fn diff_items(
    file: &str,
    names: &[&str],
    old_source: &str,
    new_source: &str,
) -> Result<Vec<String>, String> {
    let old_ast =
        syn::parse_file(old_source).map_err(|_| format!("{file}: oracle does not parse"))?;
    let new_ast = syn::parse_file(new_source).map_err(|_| format!("{file}: new does not parse"))?;
    let old_items = locate_names(file, names, &old_ast)?;
    let new_items = locate_names(file, names, &new_ast)?;
    let fields = ledger_fields(&new_ast);
    let old_lines: Vec<&str> = old_source.lines().collect();
    let new_lines: Vec<&str> = new_source.lines().collect();

    let mut violations = Vec::new();
    let old_by_key: BTreeMap<&str, &Located> =
        old_items.iter().map(|l| (l.key.as_str(), l)).collect();
    if old_by_key.len() != old_items.len() || new_items.len() != old_items.len() {
        violations.push(format!("{file}: item count differs from oracle"));
    }
    for new in &new_items {
        let Some(old) = old_by_key.get(new.key.as_str()) else {
            violations.push(format!("{file}::{}: not in oracle", new.key));
            continue;
        };
        let old_text = trimmed(&old_lines, old.first_line, old.last_line);
        let new_text = trimmed(&new_lines, new.first_line, new.last_line);
        let matched = lcs_matched(&old_text, &new_text);
        let removed = old_text.len() - matched.iter().filter(|m| **m).count();
        if removed > 0 {
            violations.push(format!(
                "{file}::{}: {removed} oracle line(s) removed or changed",
                new.key
            ));
        }

        let mut statements = LedgerStatements {
            fields: &fields,
            ranges: Vec::new(),
        };
        if let Some(block) = new.block {
            visit::visit_block(&mut statements, block);
        }
        for (offset, was_matched) in matched.iter().enumerate() {
            if *was_matched {
                continue;
            }
            let line = new.first_line + offset;
            let allowed = statements
                .ranges
                .iter()
                .any(|(a, b)| *a <= line && line <= *b);
            if !allowed {
                violations.push(format!(
                    "{file}::{}: line {line} added outside a ledger call",
                    new.key
                ));
            }
        }
    }
    Ok(violations)
}

fn trimmed<'s>(lines: &[&'s str], first: usize, last: usize) -> Vec<&'s str> {
    lines
        .get(first.saturating_sub(1)..last.min(lines.len()))
        .unwrap_or(&[])
        .iter()
        .map(|l| l.trim())
        .collect()
}

/// For each line of `new`, whether an equal line of `old` is matched by a
/// longest common subsequence. Unmatched lines of `new` are the additions.
fn lcs_matched(old: &[&str], new: &[&str]) -> Vec<bool> {
    let (n, m) = (old.len(), new.len());
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if old[i] == new[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut matched = vec![false; m];
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old[i] == new[j] {
            matched[j] = true;
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    matched
}
