//! Font discovery surface: bundled families + host fonts, listed through the
//! widget-layer `fonts` service after a render-layer `TextEnv` exists.

use pebbles_widgets::{builtins, families, has, is_builtin};

#[test]
fn discovery_lists_bundled_families_first() {
    pebbles_render::TextEnv::new();

    let fams = families();
    assert!(fams.len() >= builtins().len());
    for (i, b) in builtins().iter().enumerate() {
        assert_eq!(&fams[i], b, "bundled families lead the list in declaration order");
    }
}

#[test]
fn lookups_are_case_insensitive() {
    pebbles_render::TextEnv::new();

    assert!(has("Inter"));
    assert!(has("inter"));
    assert!(has("jetbrains mono"));
    assert!(is_builtin("Lora"));
    assert!(is_builtin("SPACE GROTESK"));
    assert!(!is_builtin("Arial"));
    assert!(!is_builtin("Not A Family"));
}

#[test]
fn bundled_names_are_stable() {
    assert_eq!(builtins(), &["Inter", "JetBrains Mono", "Space Grotesk", "Lora"]);
}
