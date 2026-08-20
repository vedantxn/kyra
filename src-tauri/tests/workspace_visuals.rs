const STYLES: &str = include_str!("../../src/styles.css");

fn rule_body(selector: &str) -> &str {
    let start = STYLES
        .find(&format!("{selector} {{"))
        .unwrap_or_else(|| panic!("missing {selector} rule"));
    let body = &STYLES[start..];
    body.split_once('}')
        .map(|(rule, _)| rule)
        .expect("unterminated CSS rule")
}

#[test]
fn native_blur_is_not_dimmed_by_a_second_full_screen_filter() {
    assert!(STYLES.contains("--workspace-tint-left: rgba(14, 16, 14, 0.42)"));
    assert!(STYLES.contains("--workspace-tint-edge: rgba(79, 75, 63, 0.14)"));
    assert!(!rule_body(".app-shell").contains("backdrop-filter"));
    assert!(!rule_body(".app-shell").contains("brightness("));
}

#[test]
fn focus_palette_and_metadata_avoid_heavy_card_and_pill_treatment() {
    assert!(rule_body(".command-palette").contains("border-radius: 12px"));
    assert!(rule_body(".command-palette").contains("background: rgba(22, 24, 21, 0.38)"));
    assert!(!rule_body(".loop-meta span").contains("border-radius"));
    assert!(!rule_body(".loop-meta span").contains("border:"));
}
