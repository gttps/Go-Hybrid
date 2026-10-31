//! closure_slot_loan_o8_uniqueness.rs — O8 uniqueness (`HYB-E3102`) inside
//! closure bodies (`check_closure_slot_loans`): a live slot loan `p := &m[k]`
//! or `p := &ns[i]` opened inside a closure body is the sole access path to
//! its slot until its last use. A second slot loan (`:=` or `=`) or a field
//! store (`ns[i].f = v` / `ns[i].f += v`) on a slot that may alias the live
//! loan is rejected with `HYB-E3102`, while provably distinct literal keys and
//! accesses after the loan's last use remain accepted.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("closure_slot_loan_o8_uniqueness.hy", src)
        .unwrap_or_else(|diags| panic!("test source must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_closure_e3102(closure_body: &str, what: &str) {
    let src = format!(
        "struct acct {{ bal int }}\n\nfn main() {{\n    m := map[string]acct{{\"a\": acct{{bal: 1}}, \"b\": acct{{bal: 2}}}}\n    ns := []acct{{acct{{bal: 1}}, acct{{bal: 2}}}}\n    f := fn() {{\n{}\n        _ = m\n        _ = ns\n    }}\n    f()\n}}\n",
        closure_body
    );
    let r = check(&src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3102"),
        "{}: expected HYB-E3102, got {:?}",
        what,
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

#[test]
fn closure_second_loan_on_same_map_and_slice_slot_is_e3102() {
    assert_closure_e3102(
        "        p := &m[\"a\"]\n        q := &m[\"a\"]\n        q.bal = 50\n        p.bal = p.bal + 1",
        "two local loans on m[\"a\"] inside closure",
    );

    assert_closure_e3102(
        "        p := &ns[0]\n        q := &ns[0]\n        q.bal = 50\n        p.bal = p.bal + 1",
        "two local loans on ns[0] inside closure",
    );

    assert_closure_e3102(
        "        k1 := \"a\"\n        k2 := \"a\"\n        p, q := &m[k1], &m[k2]\n        q.bal = 50\n        p.bal = p.bal + 1",
        "parallel slot loans with non-literal keys inside closure",
    );

    assert_closure_e3102(
        "        p := &m[\"a\"]\n        q, ok := &m[\"a\"]\n        if ok {\n            q.bal = 50\n        }\n        p.bal = p.bal + 1",
        "two-value comma-ok slot loan aliasing live slot loan inside closure",
    );
}

#[test]
fn closure_second_loan_on_assign_and_shallow_alias_is_e3102() {
    assert_closure_e3102(
        "        p := &m[\"a\"]\n        q := &m[\"b\"]\n        q.bal = 10\n        q = &m[\"a\"]\n        q.bal = 50\n        p.bal = p.bal + 1",
        "reassigned slot loan `q = &m[\"a\"]` aliasing live `p` inside closure",
    );

    assert_closure_e3102(
        "        p := &m[\"a\"]\n        q, ok := &m[\"b\"]\n        if ok {\n            q.bal = 10\n        }\n        q, ok = &m[\"a\"]\n        if ok {\n            q.bal = 50\n        }\n        p.bal = p.bal + 1",
        "reassigned two-value slot loan `q, ok = &m[\"a\"]` aliasing live `p` inside closure",
    );

    assert_closure_e3102(
        "        alias := m\n        p := &m[\"a\"]\n        q := &alias[\"a\"]\n        q.bal = 50\n        p.bal = p.bal + 1",
        "second slot loan through closure-local shallow alias",
    );
}

#[test]
fn closure_field_store_through_base_inside_slot_loan_window_is_e3102() {
    let src = "struct acct { bal int }\n\nfn main() {\n    ns := []acct{acct{bal: 1}}\n    f := fn() {\n        p := &ns[0]\n        ns[0].bal = 50\n        p.bal = p.bal + 1\n    }\n    f()\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| {
            d.code == "HYB-E3102"
                && d.message.contains("a field store on `ns` aliases the live slot loan `p`")
        }),
        "expected field-store O8 diagnostic on `ns`, got {:?}",
        r.diagnostics
    );

    assert_closure_e3102(
        "        p := &ns[0]\n        ns[0].bal += 50\n        p.bal = p.bal + 1",
        "compound field store `ns[0].bal += 50` while `p := &ns[0]` is live inside closure",
    );
}

#[test]
fn closure_distinct_slots_and_closed_windows_stay_legal() {
    let src = r#"struct acct { bal int }

fn main() {
    m := map[string]acct{"a": acct{bal: 1}, "b": acct{bal: 2}}
    ns := []acct{acct{bal: 1}, acct{bal: 2}}
    f := fn() {
        p := &m["a"]
        q := &m["b"]
        q.bal = 50
        p.bal = p.bal + 1
        x, y := &ns[0], &ns[1]
        y.bal = 8
        ns[1].bal = 9
        x.bal = x.bal + 7
        r := &m["a"]
        r.bal = 3
        s := &m["a"]
        s.bal = s.bal + 1
        s = &m["a"]
        s.bal = s.bal + 2
        ns[0].bal = 4
    }
    f()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?}", r.diagnostics);
}
