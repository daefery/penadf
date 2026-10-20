//! End-to-end checks of every document operation against the real PDFium library.
//! Run with PENADF_PDFIUM_DIR pointing at the folder holding libpdfium.so / pdfium.dll
//! (scripts/test.sh does this).

use penadf::ops::{
    self, PagePoint, PageRect, compare, create, export, forms, markup, pages, protect,
};
use penadf::{engine, page_range};

/// A document with `n` A4 pages whose text is "Page 1", "Page 2", ...
fn sample(n: usize) -> Vec<u8> {
    let text: Vec<String> = (1..=n).map(|i| format!("Page {i}")).collect();
    // from_text puts ~66 lines per page; pad each page to force breaks.
    let lines_per_page = 66;
    let mut body = String::new();
    for t in &text {
        body.push_str(t);
        body.push('\n');
        for _ in 1..lines_per_page {
            body.push('\n');
        }
    }
    let pdf = create::from_text(&body).unwrap();
    let info = ops::info(&pdf, None).unwrap();
    assert!(
        info.page_count >= n,
        "sample made {} pages, wanted {n}",
        info.page_count
    );
    pages::extract(&pdf, None, &(0..n).collect::<Vec<_>>()).unwrap()
}

fn first_lines(pdf: &[u8]) -> Vec<String> {
    ops::page_texts(pdf, None)
        .unwrap()
        .iter()
        .map(|t| t.lines().next().unwrap_or("").trim().to_string())
        .collect()
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

#[test]
fn text_to_pdf_keeps_text_and_breaks_pages() {
    let pdf = sample(3);
    assert_eq!(first_lines(&pdf), vec!["Page 1", "Page 2", "Page 3"]);
}

#[test]
fn images_to_pdf_one_page_each_sized_to_image() {
    let pdf = create::from_images(&[png(400, 200), png(100, 300)]).unwrap();
    let doc = engine::open(pdf, None).unwrap();
    let sizes = doc.pages().page_sizes().unwrap();
    assert_eq!(sizes.len(), 2);
    assert!(
        (sizes[0].width().value - 400.0).abs() < 0.5
            && (sizes[0].height().value - 200.0).abs() < 0.5
    );
    assert!((sizes[1].width().value - 100.0).abs() < 0.5);
}

#[test]
fn rotate_delete_move_extract() {
    let pdf = sample(4);

    let rotated = pages::rotate(&pdf, None, &[1], 1).unwrap();
    let rotated = pages::rotate(&rotated, None, &[1], 4).unwrap(); // full turn is a no-op
    let doc = engine::open(rotated.clone(), None).unwrap();
    assert_eq!(
        doc.pages().get(1).unwrap().rotation().unwrap(),
        pdfium_render::prelude::PdfPageRenderRotation::Degrees90
    );
    drop(doc);
    let back = pages::rotate(&rotated, None, &[1], -1).unwrap();
    let doc = engine::open(back, None).unwrap();
    assert_eq!(
        doc.pages().get(1).unwrap().rotation().unwrap(),
        pdfium_render::prelude::PdfPageRenderRotation::None
    );

    let deleted = pages::delete(&pdf, None, &[0, 2]).unwrap();
    assert_eq!(first_lines(&deleted), vec!["Page 2", "Page 4"]);
    assert!(
        pages::delete(&pdf, None, &[0, 1, 2, 3]).is_err(),
        "must keep one page"
    );

    let moved = pages::move_page(&pdf, None, 3, 0).unwrap();
    assert_eq!(
        first_lines(&moved),
        vec!["Page 4", "Page 1", "Page 2", "Page 3"]
    );
    let moved = pages::move_page(&pdf, None, 0, 3).unwrap();
    assert_eq!(
        first_lines(&moved),
        vec!["Page 2", "Page 3", "Page 4", "Page 1"]
    );
    assert!(pages::move_page(&pdf, None, 0, 4).is_err());

    let picked = pages::extract(&pdf, None, &page_range::parse("3,1", 4).unwrap()).unwrap();
    assert_eq!(first_lines(&picked), vec!["Page 3", "Page 1"]);
}

#[test]
fn merge_split_insert() {
    let a = sample(2);
    let b = pages::extract(&sample(3), None, &[2]).unwrap();
    let merged = pages::merge(&[(a.clone(), None), (b.clone(), None)]).unwrap();
    assert_eq!(first_lines(&merged), vec!["Page 1", "Page 2", "Page 3"]);
    assert!(pages::merge(&[]).is_err());

    let parts = pages::split(
        &merged,
        None,
        &page_range::parse_groups("1-2,3", 3).unwrap(),
    )
    .unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(first_lines(&parts[0]), vec!["Page 1", "Page 2"]);
    assert_eq!(first_lines(&parts[1]), vec!["Page 3"]);

    let inserted = pages::insert_document(&a, None, &b, None, 1).unwrap();
    assert_eq!(first_lines(&inserted), vec!["Page 1", "Page 3", "Page 2"]);

    let blank = pages::insert_blank(&a, None, 2).unwrap();
    assert_eq!(first_lines(&blank), vec!["Page 1", "Page 2", ""]);
}

#[test]
fn crop_sets_crop_box_and_rejects_bad_margins() {
    let pdf = sample(1);
    let cropped = pages::crop(&pdf, None, &[0], [50.0, 60.0, 70.0, 80.0]).unwrap();
    let doc = engine::open(cropped, None).unwrap();
    let page = doc.pages().get(0).unwrap();
    let media = page.boundaries().media().unwrap().bounds;
    let crop = page.boundaries().crop().unwrap().bounds;
    assert!((crop.left().value - 50.0).abs() < 0.01);
    assert!((crop.bottom().value - 60.0).abs() < 0.01);
    assert!((crop.right().value - (media.right().value - 70.0)).abs() < 0.01);
    assert!((crop.top().value - (media.top().value - 80.0)).abs() < 0.01);
    // The visible page size follows the crop box.
    assert!((page.width().value - (media.width().value - 120.0)).abs() < 0.01);
    drop(page);
    assert!(pages::crop(&pdf, None, &[0], [-1.0, 0.0, 0.0, 0.0]).is_err());
    assert!(pages::crop(&pdf, None, &[0], [300.0, 0.0, 300.0, 0.0]).is_err());
}

#[test]
fn protect_then_open_with_password_then_unprotect() {
    let pdf = sample(2);
    assert!(!protect::is_encrypted(&pdf));

    let locked = protect::protect(
        &pdf,
        None,
        "open-me",
        "owner-secret",
        protect::Allowed::default(),
    )
    .unwrap();
    assert!(protect::is_encrypted(&locked));

    let err = engine::open(locked.clone(), None).expect_err("must need a password");
    assert!(engine::is_password_error(&err));
    let doc = engine::open(locked.clone(), Some("open-me")).unwrap();
    let allowed = protect::Allowed::from_bits(engine::permission_bits(&doc).unwrap());
    assert_eq!(
        allowed,
        protect::Allowed::default(),
        "open password gives the restricted rights"
    );
    drop(doc);
    let doc = engine::open(locked.clone(), Some("owner-secret")).unwrap();
    let all = protect::Allowed {
        print: true,
        copy: true,
        edit: true,
        comment_and_fill: true,
    };
    assert_eq!(
        protect::Allowed::from_bits(engine::permission_bits(&doc).unwrap()),
        all,
        "owner password gives full rights"
    );
    drop(doc);
    assert!(engine::open(locked.clone(), Some("wrong")).is_err());

    // Editing an encrypted file keeps it encrypted.
    let rotated = pages::rotate(&locked, Some("open-me"), &[0], 1).unwrap();
    assert!(engine::is_password_error(
        &engine::open(rotated, None).err().unwrap()
    ));

    assert!(protect::unprotect(&locked, "wrong").is_err());
    let open = protect::unprotect(&locked, "owner-secret").unwrap();
    assert!(!protect::is_encrypted(&open));
    assert_eq!(first_lines(&open), vec!["Page 1", "Page 2"]);

    assert!(protect::protect(&pdf, None, "same", "same", protect::Allowed::default()).is_err());
    assert!(protect::protect(&pdf, None, "x", "", protect::Allowed::default()).is_err());
}

#[test]
fn restrictions_only_file_opens_without_password() {
    let pdf = sample(1);
    let locked = protect::protect(
        &pdf,
        None,
        "",
        "owner",
        protect::Allowed {
            print: false,
            copy: true,
            edit: false,
            comment_and_fill: false,
        },
    )
    .unwrap();
    let doc = engine::open(locked, None).unwrap();
    let allowed = protect::Allowed::from_bits(engine::permission_bits(&doc).unwrap());
    assert_eq!(
        allowed,
        protect::Allowed {
            print: false,
            copy: true,
            edit: false,
            comment_and_fill: false
        }
    );
}

#[test]
fn search_finds_text_with_rects() {
    let pdf = sample(3);
    let hits = ops::search(&pdf, None, "page 2").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].page, 1);
    assert!(!hits[0].rects.is_empty());
    assert!(ops::search(&pdf, None, "  ").unwrap().is_empty());
}

#[test]
fn highlight_note_ink_listed_and_deletable() {
    let pdf = sample(1);
    let hit = &ops::search(&pdf, None, "Page 1").unwrap()[0];
    let area = hit.rects[0];
    let lines = markup::text_lines_in(
        &pdf,
        None,
        0,
        PageRect {
            left: area.left - 2.0,
            bottom: area.bottom - 2.0,
            right: area.right + 2.0,
            top: area.top + 2.0,
        },
    )
    .unwrap();
    assert_eq!(lines.len(), 1);
    let pdf = markup::add_text_markup(
        &pdf,
        None,
        0,
        markup::TextMarkup::Highlight,
        &lines,
        markup::Rgba::YELLOW,
    )
    .unwrap();
    let pdf = markup::add_note(
        &pdf,
        None,
        0,
        PagePoint { x: 300.0, y: 500.0 },
        "Check this",
    )
    .unwrap();
    let stroke = vec![
        PagePoint { x: 100.0, y: 100.0 },
        PagePoint { x: 150.0, y: 130.0 },
    ];
    let pdf = markup::add_ink(&pdf, None, 0, &[stroke], markup::Rgba::RED, 2.0).unwrap();

    let list = markup::comments(&pdf, None).unwrap();
    let kinds: Vec<&str> = list.iter().map(|c| c.kind.as_str()).collect();
    assert_eq!(kinds, vec!["Highlight", "Text", "Ink"]);
    assert_eq!(list[1].text, "Check this");

    let pdf = markup::delete_comment(&pdf, None, 0, list[1].index).unwrap();
    let kinds: Vec<String> = markup::comments(&pdf, None)
        .unwrap()
        .into_iter()
        .map(|c| c.kind)
        .collect();
    assert_eq!(kinds, vec!["Highlight", "Ink"]);

    assert!(
        markup::add_text_markup(
            &pdf,
            None,
            0,
            markup::TextMarkup::Underline,
            &[],
            markup::Rgba::BLUE
        )
        .is_err()
    );
    assert!(markup::add_note(&pdf, None, 0, PagePoint { x: 1.0, y: 1.0 }, "  ").is_err());
}

/// Share of pixels in `r` (page points) that are clearly yellow after rendering at 72 dpi.
fn yellow_share(pdf: &[u8], r: PageRect) -> f32 {
    let doc = engine::open(pdf.to_vec(), None).unwrap();
    let img = export::render_page(&doc, 0, 72.0).unwrap();
    let h = img.height() as f32;
    let (mut yellow, mut total) = (0, 0);
    for y in (h - r.top) as u32..(h - r.bottom) as u32 {
        for x in r.left as u32..r.right as u32 {
            let p = img.get_pixel(x, y).0;
            total += 1;
            if p[0] > 200 && p[1] > 180 && p[2] < 150 {
                yellow += 1;
            }
        }
    }
    yellow as f32 / total.max(1) as f32
}

#[test]
fn highlight_paints_the_whole_line_not_a_sliver() {
    let pdf = sample(1);
    let line = ops::search(&pdf, None, "Page 1").unwrap()[0].rects[0];
    let lines = markup::text_lines_in(&pdf, None, 0, line).unwrap();
    let marked = markup::add_text_markup(
        &pdf,
        None,
        0,
        markup::TextMarkup::Highlight,
        &lines,
        markup::Rgba::YELLOW,
    )
    .unwrap();
    assert!(yellow_share(&pdf, line) < 0.01);
    let share = yellow_share(&marked, line);
    assert!(
        share > 0.5,
        "highlight covers only {:.0}% of the line",
        share * 100.0
    );
}

#[test]
fn add_text_image_and_signature_change_page_content() {
    let pdf = sample(1);
    let with_text = markup::add_text(
        &pdf,
        None,
        0,
        PagePoint { x: 72.0, y: 400.0 },
        "Approved by Fery",
        14.0,
        markup::Rgba::BLUE,
    )
    .unwrap();
    assert!(ops::page_texts(&with_text, None).unwrap()[0].contains("Approved by Fery"));

    let rect = PageRect {
        left: 300.0,
        bottom: 100.0,
        right: 500.0,
        top: 200.0,
    };
    let with_image = markup::add_image(&with_text, None, 0, rect, &png(20, 10)).unwrap();
    let sig = markup::Signature {
        strokes: vec![vec![
            PagePoint { x: 0.0, y: 0.0 },
            PagePoint { x: 1.0, y: 0.5 },
            PagePoint { x: 2.0, y: 0.0 },
        ]],
    };
    let signed = markup::sign(&with_image, None, 0, rect, &sig).unwrap();

    let doc = engine::open(signed, None).unwrap();
    use pdfium_render::prelude::PdfPageObjectsCommon;
    let page = doc.pages().get(0).unwrap();
    let objects = page.objects();
    let image_objects = objects
        .iter()
        .filter(|o| o.as_image_object().is_some())
        .count();
    let path_objects = objects
        .iter()
        .filter(|o| o.as_path_object().is_some())
        .count();
    assert_eq!(image_objects, 1);
    assert!(path_objects >= 1);
    drop(page);
    drop(doc);
    assert!(markup::sign(&pdf, None, 0, rect, &markup::Signature::default()).is_err());
}

#[test]
fn compare_two_versions() {
    let old = sample(2);
    let new = markup::add_text(
        &old,
        None,
        1,
        PagePoint { x: 72.0, y: 300.0 },
        "New clause",
        12.0,
        markup::Rgba::INK,
    )
    .unwrap();
    let report = compare::compare(&old, None, &new, None).unwrap();
    assert_eq!(report.added, 1);
    assert_eq!(report.removed, 0);
    let added: Vec<_> = report
        .lines
        .iter()
        .filter(|l| l.change == compare::Change::Added)
        .collect();
    assert_eq!((added[0].page, added[0].text.as_str()), (2, "New clause"));
}

#[test]
fn export_images_text_html() {
    let pdf = sample(2);
    let dir = std::env::temp_dir().join(format!("penadf-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let files = export::to_images(
        &pdf,
        None,
        &[0, 1],
        36.0,
        export::ImageFormat::Png,
        &dir,
        "doc",
    )
    .unwrap();
    assert_eq!(files.len(), 2);
    let img = image::open(&files[0]).unwrap();
    assert_eq!((img.width(), img.height()), (298, 421)); // A4 at 36 dpi, rounded
    let jpgs = export::to_images(
        &pdf,
        None,
        &[1],
        36.0,
        export::ImageFormat::Jpeg,
        &dir,
        "doc",
    )
    .unwrap();
    assert!(jpgs[0].extension().unwrap() == "jpg" && jpgs[0].exists());
    std::fs::remove_dir_all(&dir).unwrap();

    let text = export::to_text(&pdf, None).unwrap();
    assert!(text.contains("Page 1") && text.contains("\u{c}") && text.contains("Page 2"));

    let html = export::to_html(&pdf, None, "My <doc>").unwrap();
    assert!(html.contains("<title>My &lt;doc&gt;</title>"));
    assert_eq!(html.matches("<section").count(), 2);
    assert!(html.contains("data:image/jpeg;base64,"));
}

#[test]
fn forms_on_a_document_without_fields() {
    let pdf = sample(1);
    assert!(forms::fields(&pdf, None).unwrap().is_empty());
    // Flatten works on annotations too: a note becomes page content and stops being a comment.
    let noted = markup::add_note(&pdf, None, 0, PagePoint { x: 100.0, y: 100.0 }, "x").unwrap();
    let flat = forms::flatten(&noted, None).unwrap();
    assert!(markup::comments(&flat, None).unwrap().is_empty());
}

#[test]
fn screen_mapping_matches_pdfium_for_rotated_and_cropped_pages() {
    use pdfium_render::prelude::*;
    let pdf = sample(1);
    let pdf = pages::crop(&pdf, None, &[0], [40.0, 30.0, 20.0, 10.0]).unwrap();
    for q in 0..4 {
        let turned = pages::rotate(&pdf, None, &[0], q).unwrap();
        let doc = engine::open(turned, None).unwrap();
        let geom = penadf::geometry::page_geoms(&doc).unwrap()[0];
        let page = doc.pages().get(0).unwrap();
        let config = PdfRenderConfig::new().scale_page_by_factor(1.0);
        let bmp = page.render_with_config(&config).unwrap();
        let (w, h) = (bmp.width() as f32, bmp.height() as f32);
        let (dw, dh) = geom.display_size();
        assert!(
            (w - dw).abs() <= 1.0 && (h - dh).abs() <= 1.0,
            "q{q}: bitmap {w}x{h} vs display {dw}x{dh}"
        );
        for (u, v) in [(0.1f32, 0.2f32), (0.9, 0.7)] {
            let (px, py) = page
                .pixels_to_points((u * w) as i32, (v * h) as i32, &config)
                .unwrap();
            let mine = geom.to_page(u, v);
            assert!(
                (mine.x - px.value).abs() < 2.0 && (mine.y - py.value).abs() < 2.0,
                "q{q} ({u},{v}): mine {mine:?} pdfium ({}, {})",
                px.value,
                py.value
            );
        }
    }
}

#[test]
fn info_reports_page_count_and_encryption() {
    let pdf = sample(2);
    let info = ops::info(&pdf, None).unwrap();
    assert_eq!(info.page_count, 2);
    assert!(!info.encrypted);
    assert!(ops::info(b"not a pdf", None).is_err());
    let locked = protect::protect(&pdf, None, "", "owner", protect::Allowed::default()).unwrap();
    assert!(ops::info(&locked, None).unwrap().encrypted);
    assert!(
        ops::info(&locked, Some("owner")).unwrap().encrypted,
        "still protected when opened by the owner"
    );
}

#[test]
fn selecting_text_on_a_real_page_copies_exactly_that_text() {
    use penadf::ops::text::{self, Selection};
    let pdf = create::from_text("Harga sewa Rp 96.000.000 per tahun.\nDibayar di muka.\n\nPasal 4")
        .unwrap();
    let layout = text::layout_of(&pdf, None, 0).unwrap();
    assert!(!layout.is_empty());

    // Press on the "R" of "Rp", drag to the "u" of "muka": a selection over two lines.
    let hit = |word: &str| ops::search(&pdf, None, word).unwrap().remove(0).rects[0];
    let rp = hit("Rp");
    let muka = hit("muka");
    let start = layout
        .hit(
            PagePoint {
                x: rp.left + 1.0,
                y: (rp.bottom + rp.top) / 2.0,
            },
            2.0,
        )
        .expect("char under the press");
    let end = layout
        .nearest(PagePoint {
            x: muka.right - 1.0,
            y: (muka.bottom + muka.top) / 2.0,
        })
        .unwrap();
    let sel = Selection {
        page: 0,
        anchor: start,
        focus: end,
    };
    assert_eq!(
        layout.text(&sel),
        "Rp 96.000.000 per tahun.\nDibayar di muka"
    );
    assert_eq!(
        layout.highlight(&sel).len(),
        2,
        "one highlight bar per line"
    );

    // Double-click a word; Ctrl+A takes the whole page.
    assert_eq!(layout.text(&layout.word_at(0, start).unwrap()), "Rp");
    // A PDF has no characters for an empty line, so the blank line between paragraphs
    // becomes one line break (as when copying from Chrome or Acrobat).
    let all = layout.text(&layout.select_all(0).unwrap());
    assert_eq!(
        all,
        "Harga sewa Rp 96.000.000 per tahun.\nDibayar di muka.\nPasal 4"
    );
}
