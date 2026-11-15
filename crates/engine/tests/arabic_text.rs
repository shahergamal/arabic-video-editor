//! Arabic must always come out right: joined letters, ligatures, harakat,
//! right-to-left order, and mixed Arabic/English lines. Checked for every
//! bundled font.

mod common;
use engine::text::TextRenderer;

fn renderer() -> TextRenderer {
    TextRenderer::new(Some(&common::fonts_dir()))
}

fn bundled_families() -> Vec<String> {
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(common::fonts_dir().join("manifest.json")).unwrap()).unwrap();
    manifest["fonts"].as_array().unwrap().iter().map(|f| f["family"].as_str().unwrap().to_string()).collect()
}

#[test]
fn every_bundled_font_is_loaded() {
    let r = renderer();
    for family in bundled_families() {
        assert!(r.has_family(&family), "bundled font not found: {family}");
    }
}

#[test]
fn letters_join_into_initial_medial_final_forms() {
    let mut r = renderer();
    for family in bundled_families() {
        let isolated = r.shape("ب", &family, 48.0)[0].glyph_id;
        let joined = r.shape("ببب", &family, 48.0);
        assert_eq!(joined.len(), 3, "{family}");
        // Sort glyphs back into reading order (by where they came from in the text).
        let mut by_text = joined.clone();
        by_text.sort_by_key(|g| g.start);
        let (first, middle, last) = (by_text[0].glyph_id, by_text[1].glyph_id, by_text[2].glyph_id);
        assert_ne!(first, isolated, "{family}: first letter not in initial form");
        assert_ne!(last, isolated, "{family}: last letter not in final form");
        assert_ne!(first, last, "{family}: initial and final forms are the same glyph");
        assert_ne!(middle, first, "{family}: medial form missing");
    }
}

#[test]
fn arabic_reads_right_to_left() {
    let mut r = renderer();
    for family in bundled_families() {
        let glyphs = r.shape("سلام", &family, 48.0);
        assert!(glyphs.iter().all(|g| g.right_to_left), "{family}");
        let first_letter = glyphs.iter().find(|g| g.start == 0).unwrap();
        let max_x = glyphs.iter().map(|g| g.x).fold(f32::MIN, f32::max);
        assert_eq!(first_letter.x, max_x, "{family}: first letter is not on the right");
    }
}

#[test]
fn lam_alef_is_joined_not_two_separate_letters() {
    // Some fonts draw لا as one glyph, others (like Amiri) as two joined
    // pieces. Either way, neither piece may be the stand-alone ل or ا.
    let mut r = renderer();
    for family in bundled_families() {
        let lam = r.shape("ل", &family, 48.0)[0].glyph_id;
        let alef = r.shape("ا", &family, 48.0)[0].glyph_id;
        let glyphs = r.shape("لا", &family, 48.0);
        assert!(!glyphs.is_empty() && glyphs.len() <= 2, "{family}");
        assert!(
            glyphs.iter().all(|g| g.glyph_id != lam && g.glyph_id != alef),
            "{family}: لا was drawn as two separate letters"
        );
    }
}

#[test]
fn harakat_are_kept() {
    let mut r = renderer();
    for family in bundled_families() {
        let plain = r.shape("كتب", &family, 48.0);
        let marked = r.shape("كَتَبَ", &family, 48.0);
        assert!(marked.len() > plain.len(), "{family}: harakat were dropped");
    }
}

#[test]
fn no_missing_glyph_boxes() {
    let mut r = renderer();
    let sample = "بسم الله الرحمن الرحيم ١٢٣ 123 «مرحباً» ؟،";
    for family in bundled_families() {
        let glyphs = r.shape(sample, &family, 48.0);
        assert!(glyphs.iter().all(|g| g.glyph_id != 0), "{family}: a character has no glyph");
    }
}

#[test]
fn english_inside_arabic_stays_left_to_right() {
    let mut r = renderer();
    let text = "عرض Premium الآن";
    let glyphs = r.shape(text, "IBM Plex Sans Arabic", 48.0);
    let start = text.find("Premium").unwrap();
    let latin: Vec<_> = glyphs.iter().filter(|g| g.start >= start && g.start < start + 7).collect();
    assert_eq!(latin.len(), 7);
    assert!(latin.iter().all(|g| !g.right_to_left));
    let p = latin.iter().find(|g| g.start == start).unwrap();
    let m = latin.iter().find(|g| g.start == start + 6).unwrap();
    // "P" sits left of the final "m" inside the English word.
    assert!(p.x < m.x);
    // The first Arabic word is to the right of the English word.
    let first_ar = glyphs.iter().find(|g| g.start == 0).unwrap();
    assert!(first_ar.x > m.x);
}

