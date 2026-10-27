//! R8 hardening (QA P3 H1, H2, H4, H5; research plan R8 ASUMSI): a hostile upload costs one
//! 422 and never the server; the rule scanner is linear in the length of a line; numbers that
//! are not figures (footnote marks, page references, dates, times) are skipped and space or
//! no-break-space thousands read right; and accepting one candidate of a KPI cell settles its
//! siblings. Every file is generated here; nothing is real data.

#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/r4_env.rs"]
mod r4_env;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use std::io::Write;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use investing_app::research::extract::doc::{DocError, Format};
use investing_app::research::extract::isolated::{self, Limits};
use investing_app::research::extract::rules::{propose, Target};
use http_body_util::BodyExt;
use r4_env::{env, Env};
use serde_json::{json, Value};
use tower::ServiceExt;

const BOUNDARY: &str = "----r8-hardening-boundary";

// ---- synthetic documents ----

/// A PDF from raw object bodies (object 1 is the catalog).
fn build_pdf(objs: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n{:010} 65535 f \n", objs.len() + 1, 0).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn text_content(lines: &[&str]) -> String {
    let mut c = String::from("BT /F1 12 Tf 72 720 Td 18 TL\n");
    for l in lines {
        c.push_str(&format!("({l}) Tj T*\n"));
    }
    c.push_str("ET");
    c
}

fn stream_obj(content: &str) -> String {
    format!(
        "<< /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    )
}

const PAGE_DICT: &str = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents";

/// A readable one-page PDF.
fn good_pdf(lines: &[&str]) -> Vec<u8> {
    build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [4 0 R] /Count 1 >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("{PAGE_DICT} 5 0 R >>"),
        stream_obj(&text_content(lines)),
    ])
}

/// QA H1.7: a Form XObject whose content draws itself. `pdf-extract` recurses on every `Do`
/// until the stack is gone.
fn self_referencing_xobject_pdf() -> Vec<u8> {
    let c = "q /X1 Do Q";
    let fc = "/X1 Do";
    build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /X1 5 0 R >> >> /Contents 4 0 R >>".into(),
        stream_obj(c),
        format!("<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Resources << /XObject << /X1 5 0 R >> >> /Length {} >>\nstream\n{fc}\nendstream", fc.len()),
    ])
}

/// QA H1.8: a FlateDecode content stream that inflates to `mib` MiB of spaces.
fn flate_bomb_pdf(mib: usize) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    let chunk = vec![b' '; 1 << 20];
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::fast());
    for _ in 0..mib {
        enc.write_all(&chunk).unwrap();
    }
    let data = enc.finish().unwrap();
    let head = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [4 0 R] /Count 1 >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("{PAGE_DICT} 5 0 R >>"),
    ];
    // A binary stream: assembled by hand, the string helper is for text.
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in head.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    offsets.push(out.len());
    out.extend_from_slice(
        format!(
            "5 0 obj\n<< /Length {} /Filter /FlateDecode >>\nstream\n",
            data.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(&data);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 6\n{:010} 65535 f \n", 0).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(b"trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n");
    out.extend_from_slice(format!("{xref}\n%%EOF\n").as_bytes());
    out
}

/// One slide whose single paragraph is `text`.
fn pptx_paragraph(text: &str) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        let mut put = |name: &str, body: String| {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        };
        put(
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#
                .into(),
        );
        put(
            "ppt/presentation.xml",
            r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst></p:presentation>"#.into(),
        );
        put(
            "ppt/_rels/presentation.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="slide" Target="slides/slide1.xml"/></Relationships>"#.into(),
        );
        put(
            "ppt/slides/slide1.xml",
            format!(
                r#"<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#
            ),
        );
        zip.finish().unwrap();
    }
    buf.into_inner()
}

// ---- requests ----

fn multipart(fields: &[(&str, &str)], file: (&str, &[u8])) -> Vec<u8> {
    let mut body = Vec::new();
    for (k, v) in fields {
        body.extend_from_slice(
            format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n")
                .as_bytes(),
        );
    }
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: application/octet-stream\r\n\r\n",
            file.0
        )
        .as_bytes(),
    );
    body.extend_from_slice(file.1);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

const FORM: [(&str, &str); 4] = [
    ("kind", "presentasi"),
    ("doc_date", "2026-08-20"),
    ("fy", "2026"),
    ("q", "2"),
];

async fn upload(e: &Env, key: &str, name: &str, bytes: &[u8]) -> (StatusCode, Value, Duration) {
    let t0 = Instant::now();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/research/COAL/documents/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .header("idempotency-key", key)
        .body(Body::from(multipart(&FORM, (name, bytes))))
        .unwrap();
    let res = e.app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&body).unwrap_or(Value::Null),
        t0.elapsed(),
    )
}

/// The server answered, in time, with a 422 `file_unreadable`, and still serves a good file.
async fn refused_and_still_serving(e: &Env, label: &str, bad: &[u8], within: Duration) {
    let (status, body, took) = upload(e, &format!("bad-{label}"), "bad.pdf", bad).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{label}: {body}");
    assert_eq!(
        body["detail"][0]["loc"],
        json!(["body", "file"]),
        "{label}: {body}"
    );
    assert_eq!(
        body["detail"][0]["type"], "file_unreadable",
        "{label}: {body}"
    );
    assert!(took < within, "{label}: took {took:?}");
    let ok = good_pdf(&["Produksi 12,5 juta ton"]);
    let (s, v, _) = upload(e, &format!("ok-{label}"), "ok.pdf", &ok).await;
    assert_eq!(
        s,
        StatusCode::CREATED,
        "{label}: server must keep serving: {v}"
    );
    assert_eq!(v["proposals"][0]["value"], 12.5);
}

// ---- H1: a hostile PDF stays in the child ----

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_self_referencing_form_xobject_is_422_and_the_server_keeps_serving() {
    let e = env();
    refused_and_still_serving(
        &e,
        "xobject",
        &self_referencing_xobject_pdf(),
        Duration::from_secs(30),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_flate_bomb_is_stopped_by_the_memory_limit_and_answers_422() {
    let e = env();
    // 600 MiB inflated is far past the default 512 MiB limit; on the wire it is under 3 MB.
    let bomb = flate_bomb_pdf(600);
    assert!(bomb.len() < 4_000_000, "{}", bomb.len());
    refused_and_still_serving(&e, "bomb", &bomb, Duration::from_secs(30)).await;
}

#[tokio::test]
async fn the_child_stops_a_bomb_under_a_small_memory_limit() {
    let bomb = flate_bomb_pdf(200);
    let limits = Limits {
        max_rss_mb: 64,
        ..Limits::default()
    };
    let t0 = Instant::now();
    let got = isolated::pages(Format::Pdf, bomb, &limits).await;
    assert!(matches!(got, Err(DocError::Unreadable(_))), "{got:?}");
    assert!(t0.elapsed() < Duration::from_secs(20), "{:?}", t0.elapsed());
}

#[tokio::test]
async fn the_wall_clock_budget_and_the_output_cap_stop_a_child() {
    let pdf = good_pdf(&["Produksi 12,5 juta ton"]);
    // Read normally first, so the limits below are the cause.
    let pages = isolated::pages(Format::Pdf, pdf.clone(), &Limits::default())
        .await
        .unwrap();
    assert!(pages[0].contains("Produksi"));
    let too_slow = Limits {
        timeout: Duration::from_millis(1),
        ..Limits::default()
    };
    assert!(matches!(
        isolated::pages(Format::Pdf, pdf.clone(), &too_slow).await,
        Err(DocError::Unreadable(_))
    ));
    let too_big = Limits {
        max_output: 8,
        ..Limits::default()
    };
    assert!(matches!(
        isolated::pages(Format::Pdf, pdf, &too_big).await,
        Err(DocError::Unreadable(_))
    ));
}

#[tokio::test]
async fn the_child_keeps_the_error_kinds_of_the_in_process_reader() {
    let l = Limits::default();
    assert!(matches!(
        isolated::pages(Format::Pdf, b"%PDF-1.4 truncated".to_vec(), &l).await,
        Err(DocError::Unreadable("The file is not a readable PDF"))
    ));
    assert!(matches!(
        isolated::pages(Format::Pptx, b"PK\x03\x04 nope".to_vec(), &l).await,
        Err(DocError::Unreadable(
            "The file is not a readable PPTX presentation"
        ))
    ));
}

// ---- H2: the scanner is linear in the length of a line ----

fn coal_targets() -> Vec<Target> {
    vec![Target {
        metric: "production_mt".into(),
        label: "Produksi".into(),
        aliases: vec!["coal production".into()],
        unit: "Mt".into(),
    }]
}

#[test]
fn one_very_long_line_is_scanned_in_linear_time() {
    for (unit, label) in [
        ("Produksi 12,5 Mt ", "label and figure"),
        ("Produksi ", "label only"),
        ("12,5 Mt ", "figures only"),
        (
            "Rp 12 345 miliar Produksi (1) ",
            "currency, spaced thousands, marks",
        ),
    ] {
        let mut times = Vec::new();
        for reps in [10_000usize, 20_000, 80_000] {
            let page = unit.repeat(reps);
            let t0 = Instant::now();
            let got = propose(&coal_targets(), &[page], &|_| true);
            let took = t0.elapsed();
            assert!(got.len() <= 3, "{label}");
            times.push((reps, took));
            // Over a megabyte of text; the quadratic scanner needed minutes for a third of it.
            assert!(
                took < Duration::from_secs(10),
                "{label}: {reps} repeats took {took:?}"
            );
        }
        // Eight times the text must not cost anywhere near sixty-four times the time.
        let (small, large) = (
            times[0].1.as_secs_f64().max(0.002),
            times[2].1.as_secs_f64(),
        );
        assert!(large < small * 40.0, "{label}: {times:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_340_kb_paragraph_is_answered_within_the_budget() {
    let e = env();
    let text = "Produksi 12,5 Mt ".repeat(20_000);
    let (status, body, took) = upload(&e, "long", "long.pptx", &pptx_paragraph(&text)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert!(took < Duration::from_secs(30), "{took:?}");
    assert_eq!(body["proposals"][0]["value"], 12.5);
}

// ---- H4: the number parser ----

#[test]
fn the_h4_table_gives_the_right_figure_or_none() {
    let t = |m: &str, l: &str, a: &[&str], u: &str| Target {
        metric: m.into(),
        label: l.into(),
        aliases: a.iter().map(|s| s.to_string()).collect(),
        unit: u.into(),
    };
    let targets = vec![
        t("production_mt", "Produksi", &[], "Mt"),
        t("margin", "Margin", &[], "%"),
        t("revenue", "Pendapatan", &[], "IDR miliar"),
        t("asp_usd_t", "Harga jual rata-rata", &["ASP"], "USD/t"),
        t("vessels", "Armada", &[], "kapal"),
    ];
    // (page text, the value to propose or None, the other legitimate reading, note)
    let rows: Vec<(&str, Option<f64>, Option<f64>, &str)> = vec![
        (
            "Produksi 1.234",
            Some(1234.0),
            Some(1.234),
            "Indonesian thousands vs English decimal; ASUMSI says thousands",
        ),
        (
            "Produksi 1,234",
            Some(1234.0),
            Some(1.234),
            "English thousands vs Indonesian decimal; ASUMSI says thousands",
        ),
        ("Produksi 1.234,5", Some(1234.5), None, ""),
        ("Produksi 1,234.5", Some(1234.5), None, ""),
        ("Produksi (12,5)", Some(-12.5), None, "accounting negative"),
        ("Produksi -12", Some(-12.0), None, ""),
        ("Produksi \u{2212}12", Some(-12.0), None, "U+2212 minus"),
        ("Produksi 12%", None, None, "percent for a Mt KPI"),
        ("Produksi 12 %", None, None, "percent for a Mt KPI, spaced"),
        ("Margin 12%", Some(12.0), None, ""),
        ("Margin 12 %", Some(12.0), None, ""),
        ("Margin 12,5%", Some(12.5), None, ""),
        (
            "Margin (3,5)%",
            Some(-3.5),
            None,
            "accounting negative percent",
        ),
        ("Margin \u{2212}3,5%", Some(-3.5), None, "U+2212 percent"),
        ("Margin 0,5%", Some(0.5), None, ""),
        (
            "Margin 1.234%",
            Some(1.234),
            Some(1234.0),
            "a percentage with a thousands-looking group",
        ),
        ("Produksi 1.234.567", Some(1_234_567.0), None, ""),
        ("Produksi 1,234,567.89", Some(1_234_567.89), None, ""),
        ("Produksi 0,5", Some(0.5), None, ""),
        (
            "Produksi 5.4 juta",
            Some(5.4),
            None,
            "juta + implied tonnes -> Mt",
        ),
        ("Produksi 5,4 juta ton", Some(5.4), None, ""),
        (
            "Pendapatan Rp 1,2 triliun",
            Some(1200.0),
            None,
            "IDR miliar KPI",
        ),
        ("Pendapatan Rp 1.234 miliar", Some(1234.0), None, ""),
        (
            "Pendapatan Rp 1,234 triliun",
            Some(1234.0),
            Some(1_234_000.0),
            "ID decimal 1,234 triliun = 1.234 triliun = 1234 miliar",
        ),
        ("Pendapatan Rp 5.400 miliar", Some(5400.0), None, ""),
        ("Pendapatan IDR 5.6 trillion", Some(5600.0), None, ""),
        (
            "Pendapatan 12.000 juta",
            Some(12.0),
            None,
            "no currency word",
        ),
        ("Pendapatan Rp 950 juta", Some(0.95), None, ""),
        ("Produksi 12.000 ton", Some(0.012), None, ""),
        ("Produksi 1,2 Mt", Some(1.2), None, ""),
        ("Produksi 12.500 kt", Some(12.5), None, ""),
        ("Produksi 1.2 million tonnes", Some(1.2), None, ""),
        ("Produksi n/a", None, None, ""),
        ("Produksi -", None, None, ""),
        ("Produksi \u{2014}", None, None, "em dash"),
        ("Produksi \u{2013}", None, None, "en dash"),
        (
            "Produksi FY2025",
            None,
            None,
            "a year must never be a value",
        ),
        ("Produksi 2025", None, None, "a bare year"),
        ("Produksi 3Q25", None, None, ""),
        ("Produksi 2Q26: 12,5 Mt", Some(12.5), None, ""),
        ("Produksi FY2025 12,5 Mt", Some(12.5), None, ""),
        (
            "Produksi 2025 12,5 Mt",
            Some(12.5),
            None,
            "year before the figure",
        ),
        (
            "Produksi 2025: 12,5 Mt vs 2024: 11,0 Mt",
            Some(12.5),
            None,
            "",
        ),
        ("Produksi Juni 2026 12,5 Mt", Some(12.5), None, ""),
        ("Produksi pada 2026 sebesar 12,5 Mt", Some(12.5), None, ""),
        (
            "Produksi 2026 juta ton",
            Some(2026.0),
            None,
            "year-like number that IS a value (has a scale)",
        ),
        ("Armada 2025", None, None, "plain-unit KPI and a year"),
        ("Armada FY2025 18 kapal", Some(18.0), None, ""),
        (
            "Armada 2.025",
            Some(2025.0),
            None,
            "thousands-grouped, not a year",
        ),
        (
            "Produksi 12,5 Mt (1)",
            Some(12.5),
            None,
            "footnote marker after",
        ),
        (
            "Produksi (1) 12,5 Mt",
            Some(12.5),
            None,
            "footnote marker in parentheses before the figure",
        ),
        (
            "Produksi\u{00b9} 12,5 Mt",
            Some(12.5),
            None,
            "superscript one",
        ),
        ("Produksi*) 12,5 Mt", Some(12.5), None, ""),
        ("Produksi 1) 12,5 Mt", Some(12.5), None, "numbered footnote"),
        ("Produksi [1] 12,5 Mt", Some(12.5), None, ""),
        ("Produksi 12,5 Mt 15", Some(12.5), None, "page number after"),
        (
            "Produksi hal. 15 12,5 Mt",
            Some(12.5),
            None,
            "Indonesian 'hal.' page marker",
        ),
        (
            "Produksi p. 15 12,5 Mt",
            Some(12.5),
            None,
            "'p.' page marker",
        ),
        ("Produksi halaman 15 12,5 Mt", Some(12.5), None, ""),
        ("Produksi page 15 12,5 Mt", Some(12.5), None, ""),
        (
            "Produksi 30 Juni 2026 12,5 Mt",
            Some(12.5),
            None,
            "day of month before the figure",
        ),
        (
            "Produksi 30/06/2026 12,5 Mt",
            Some(12.5),
            None,
            "numeric date",
        ),
        (
            "Produksi 30-06-2026 12,5 Mt",
            Some(12.5),
            None,
            "numeric date with dashes",
        ),
        ("Produksi 2026-06-30 12,5 Mt", Some(12.5), None, "ISO date"),
        ("Produksi Q2 12,5 Mt", Some(12.5), None, ""),
        ("Produksi 6M26 12,5 Mt", Some(12.5), None, "half-year token"),
        (
            "Produksi 12,5 11,0 10,2",
            Some(12.5),
            None,
            "three comparison columns, first is current",
        ),
        ("Produksi YoY +5,2% 12,5 Mt", Some(12.5), None, ""),
        (
            "Produksi\n12,5 Mt\n15",
            Some(12.5),
            None,
            "label alone, figure next line, page number after",
        ),
        (
            "Produksi\n15",
            None,
            None,
            "label alone then a page number: indistinguishable from a value",
        ),
        ("Produksi batubara\nHalaman 15", None, None, ""),
        (
            "15\nProduksi\n12,5 Mt",
            Some(12.5),
            None,
            "page number above",
        ),
        (
            "1. Produksi 12,5 Mt",
            Some(12.5),
            None,
            "list numbering before the label",
        ),
        ("Produksi 1.2.3", None, None, ""),
        ("Produksi 12,5,", Some(12.5), None, "trailing comma"),
        ("ASP USD 65,3/t", Some(65.3), None, ""),
        ("ASP US$ 65.3 per ton", Some(65.3), None, ""),
        ("ASP $65.3/t", Some(65.3), None, ""),
        (
            "ASP Rp 950.000/ton",
            None,
            None,
            "wrong currency for a USD KPI",
        ),
        ("ASP 1.234", Some(1234.0), Some(1.234), "bare, ambiguous"),
        ("Armada 12 kapal", Some(12.0), None, ""),
        ("Armada 1,5 kapal", Some(1.5), None, ""),
        (
            "Produksi 1 234,5 Mt",
            Some(1234.5),
            None,
            "space as thousands separator",
        ),
        (
            "Produksi 1\u{00a0}234,5 Mt",
            Some(1234.5),
            None,
            "no-break space as thousands separator",
        ),
        (
            "Pendapatan Rp 12 345 miliar",
            Some(12345.0),
            None,
            "space thousands, IDR",
        ),
        (
            "Produksi 12,5Mt",
            Some(12.5),
            None,
            "unit glued to the number",
        ),
        (
            "Produksi 12,5-13,0 Mt",
            None,
            Some(12.5),
            "range: neither end is 'the' value",
        ),
        ("Produksi ~12,5 Mt", Some(12.5), None, "approximation sign"),
        ("Produksi +12,5 Mt", Some(12.5), None, "explicit plus"),
        (
            "Produksi -12,5 Mt",
            Some(-12.5),
            None,
            "hyphen-minus before decimal",
        ),
        ("Produksi Rp -", None, None, ""),
        (
            "Produksi 12,5 Mt\u{2191}5%",
            Some(12.5),
            None,
            "arrow glyph",
        ),
        (
            "Produksi 1.234 Mt",
            Some(1234.0),
            Some(1.234),
            "ID reading 1.234 Mt = 1234 Mt vs EN 1.234 Mt",
        ),
        ("Produksi 0.125 Mt", Some(0.125), None, ""),
        (
            "Produksi 100.000 ton",
            Some(0.1),
            None,
            "ID thousands with unit ton",
        ),
        (
            "Produksi 100,000 tonnes",
            Some(0.1),
            None,
            "EN thousands with unit tonnes",
        ),
        ("Produksi 1,5 juta", Some(1.5), None, ""),
        (
            "Pendapatan 1,5 triliun",
            Some(1500.0),
            None,
            "no currency word",
        ),
        (
            "Pendapatan USD 1,5 miliar",
            None,
            None,
            "USD value for an IDR KPI",
        ),
    ];
    let eq = |a: Option<f64>, b: Option<f64>| match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => (x - y).abs() <= 1e-9 * y.abs().max(1.0),
        _ => false,
    };
    let mut wrong = Vec::new();
    for (text, correct, alt, note) in &rows {
        let pages = vec![text.to_string()];
        let got = propose(&targets, &pages, &|_| true);
        let parsed = got.iter().find(|p| p.rank == 1).map(|p| p.value);
        if !(eq(parsed, *correct) || (alt.is_some() && eq(parsed, *alt))) {
            wrong.push(format!(
                "{text:?}: parsed {parsed:?}, want {correct:?} ({note})"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} rows wrong:\n{}",
        wrong.len(),
        rows.len(),
        wrong.join("\n")
    );
    assert_eq!(rows.len(), 99);
}

fn first_value(unit: &str, label: &str, text: &str) -> Option<f64> {
    let targets = vec![Target {
        metric: "m".into(),
        label: label.into(),
        aliases: vec![],
        unit: unit.into(),
    }];
    propose(&targets, &[text.to_string()], &|_| true)
        .first()
        .map(|p| p.value)
}

#[test]
fn marks_page_references_dates_and_times_are_skipped_not_misread() {
    for (text, want) in [
        // Footnote marks: a bare one or two digit integer in brackets or before a ")".
        ("Produksi (2) 12,5 Mt", Some(12.5)),
        ("Produksi 12) 12,5 Mt", Some(12.5)),
        ("Produksi [12] 12,5 Mt", Some(12.5)),
        ("Produksi (1)", None),
        // A decimal or a three digit figure in parentheses is an accounting negative.
        ("Produksi (100)", Some(-100.0)),
        ("Produksi (12,5) Mt", Some(-12.5)),
        // Page references.
        ("Produksi hlm 7 12,5 Mt", Some(12.5)),
        ("Produksi hlm. 7 12,5 Mt", Some(12.5)),
        ("Produksi pp. 7 12,5 Mt", Some(12.5)),
        ("Produksi p.15 12,5 Mt", Some(12.5)),
        ("Produksi Hal. 15 12,5 Mt", Some(12.5)),
        // Dates.
        ("Produksi 30 Juni 2026 12,5 Mt", Some(12.5)),
        ("Produksi 5 Mei 26 12,5 Mt", Some(12.5)),
        ("Produksi 30-Jun-2026 12,5 Mt", Some(12.5)),
        ("Produksi Juni 30, 2026 12,5 Mt", Some(12.5)),
        ("Produksi 30.06.2026 12,5 Mt", Some(12.5)),
        ("Produksi 30-06-26 12,5 Mt", Some(12.5)),
        ("Produksi 2026-06-30 12,5 Mt", Some(12.5)),
        ("Produksi 2026/06/30 12,5 Mt", Some(12.5)),
        ("Produksi 2026-06-30", None),
        ("Produksi 30 Juni 2026", None),
        // A time of day.
        ("Produksi 12:30 12,5 Mt", Some(12.5)),
        // A number that merely looks like a date is still a number.
        ("Produksi 1.234.567", Some(1_234_567.0)),
        ("Produksi 12 Mt pada 30 Juni 2026", Some(12.0)),
        // Thousands separated by a space or a no-break space.
        ("Produksi 1 234,5 Mt", Some(1234.5)),
        ("Produksi 1\u{a0}234,5 Mt", Some(1234.5)),
        ("Produksi 1\u{202f}234 Mt", Some(1234.0)),
        ("Produksi 12 345 678 ton", Some(12.345678)),
        ("Produksi 12 345,67 Mt 11 111,1 Mt", Some(12345.67)),
        // Two digits after a space are not a thousands group.
        ("Produksi 12 34 Mt", Some(12.0)),
    ] {
        let got = first_value("Mt", "Produksi", text);
        assert_eq!(got, want, "{text:?}");
    }
    assert_eq!(
        first_value("IDR miliar", "Pendapatan", "Pendapatan Rp 12 345 miliar"),
        Some(12345.0)
    );
    assert_eq!(
        first_value(
            "IDR miliar",
            "Pendapatan",
            "Pendapatan Rp 1 234,5 miliar (1)"
        ),
        Some(1234.5)
    );
    // A label alone, then a bare small number on the last line: a page number.
    assert_eq!(first_value("Mt", "Produksi", "Produksi\n15"), None);
    assert_eq!(
        first_value("Mt", "Produksi", "Produksi\n15\nCatatan"),
        Some(15.0)
    );
    assert_eq!(first_value("Mt", "Produksi", "Produksi\n15 Mt"), Some(15.0));
}

#[test]
fn a_superscript_mark_after_a_label_does_not_hide_it() {
    assert_eq!(
        first_value("Mt", "Produksi", "Produksi\u{b9} 12,5 Mt"),
        Some(12.5)
    );
    assert_eq!(
        first_value("Mt", "Produksi", "Produksi\u{b2}\u{b3} 12,5 Mt"),
        Some(12.5)
    );
}

// ---- H5: one decision per KPI cell ----

/// Two pages, so the production cell has two candidates: 12,5 and 13,0.
fn two_candidates_pdf() -> Vec<u8> {
    build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [4 0 R 6 0 R] /Count 2 >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("{PAGE_DICT} 5 0 R >>"),
        stream_obj(&text_content(&["Produksi 12,5 juta ton"])),
        format!("{PAGE_DICT} 7 0 R >>"),
        stream_obj(&text_content(&["Produksi 13,0 juta ton"])),
    ])
}

fn ids(v: &Value) -> Vec<(String, f64, String)> {
    v["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["metric"] == "production_mt")
        .map(|p| {
            (
                p["id"].as_str().unwrap().to_string(),
                p["value"].as_f64().unwrap(),
                p["status"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

async fn accept(e: &Env, id: &str, key: &str, replace: bool) -> (StatusCode, Value) {
    e.post(
        &format!("/api/research/proposals/{id}/accept"),
        key,
        if replace {
            json!({"replace": true})
        } else {
            json!({})
        },
    )
    .await
}

fn statuses(e: &Env) -> Vec<(f64, String)> {
    let mut v: Vec<(f64, String)> = e
        .store
        .proposals("COAL", None, None)
        .unwrap()
        .into_iter()
        .filter(|p| p.kpi_key.starts_with("production_mt"))
        .map(|p| (p.value, p.status))
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

#[tokio::test]
async fn accepting_a_candidate_supersedes_its_pending_siblings() {
    let e = env();
    let (s, v, _) = upload(&e, "u1", "a.pdf", &two_candidates_pdf()).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let c = ids(&v);
    assert_eq!(c.len(), 2, "{c:?}");
    let first = c.iter().find(|x| x.1 == 12.5).unwrap().0.clone();
    let second = c.iter().find(|x| x.1 == 13.0).unwrap().0.clone();
    let (s, a) = accept(&e, &first, "k1", false).await;
    assert_eq!(s, StatusCode::OK, "{a}");
    assert_eq!(a["superseded"], json!([second.clone()]));
    assert_eq!(
        statuses(&e),
        [
            (12.5, "accepted".to_string()),
            (13.0, "superseded".to_string())
        ]
    );
    // The sibling is decided: it can no longer be accepted (with or without replace) or
    // rejected, so the cell and the proposal list cannot drift apart.
    for (i, (what, replace)) in [("accept", false), ("accept", true), ("reject", false)]
        .into_iter()
        .enumerate()
    {
        let (s, v) = e
            .post(
                &format!("/api/research/proposals/{second}/{what}"),
                &format!("late{i}"),
                if replace {
                    json!({"replace": true})
                } else {
                    json!({})
                },
            )
            .await;
        assert_eq!(s, StatusCode::CONFLICT, "{what}: {v}");
        assert_eq!(v["detail"], "The proposal was already superseded");
    }
    let (_, list) = e
        .get("/api/research/COAL/proposals?status=superseded")
        .await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["counts"]["superseded"], 1);
    let kpi = e.store.kpis("COAL").unwrap();
    assert_eq!(
        kpi.iter().filter(|k| k.metric == "production_mt").count(),
        1
    );
    assert_eq!(
        kpi.iter()
            .find(|k| k.metric == "production_mt")
            .unwrap()
            .value,
        Some(12.5)
    );
}

#[tokio::test]
async fn a_replace_from_another_document_supersedes_the_value_it_overwrote() {
    let e = env();
    let (_, v1, _) = upload(&e, "u1", "a.pdf", &two_candidates_pdf()).await;
    let first = ids(&v1).into_iter().find(|x| x.1 == 12.5).unwrap().0;
    let (s, _) = accept(&e, &first, "k1", false).await;
    assert_eq!(s, StatusCode::OK);
    // A second report for the same period offers 14,0.
    let other = good_pdf(&["Produksi 14,0 juta ton"]);
    let (_, v2, _) = upload(&e, "u2", "b.pdf", &other).await;
    let newer = ids(&v2).into_iter().find(|x| x.1 == 14.0).unwrap().0;
    let (s, v) = accept(&e, &newer, "k2", false).await;
    assert_eq!(s, StatusCode::CONFLICT, "{v}");
    let (s, v) = accept(&e, &newer, "k3", true).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["replaced"], true);
    assert_eq!(v["superseded"], json!([first]));
    // The value of the cell is the one accepted last; the one it replaced no longer reads
    // `accepted`, and the first document's other candidate stays superseded.
    assert_eq!(
        statuses(&e),
        [
            (12.5, "superseded".to_string()),
            (13.0, "superseded".to_string()),
            (14.0, "accepted".to_string())
        ]
    );
    let held = e.store.kpis("COAL").unwrap();
    assert_eq!(
        held.iter()
            .find(|k| k.metric == "production_mt")
            .unwrap()
            .value,
        Some(14.0)
    );
}

#[tokio::test]
async fn a_replace_over_an_owner_value_settles_only_the_siblings() {
    let e = env();
    let (_, v, _) = upload(&e, "u1", "a.pdf", &two_candidates_pdf()).await;
    let first = ids(&v).into_iter().find(|x| x.1 == 12.5).unwrap().0;
    let put = e
        .send(
            Method::PUT,
            "/api/research/COAL/kpi/production_mt/2026/2",
            "pk",
            json!({"value": 9.9, "document": "owner", "document_date": "2026-10-01"}),
        )
        .await;
    assert_eq!(put.0, StatusCode::OK, "{}", put.1);
    let (s, a) = accept(&e, &first, "k1", true).await;
    assert_eq!(s, StatusCode::OK, "{a}");
    assert_eq!(a["replaced"], true);
    // Only the sibling of the same document is settled; nothing was accepted before.
    assert_eq!(a["superseded"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn rebuild_replays_superseded_states() {
    let e = env();
    let (_, v, _) = upload(&e, "u1", "a.pdf", &two_candidates_pdf()).await;
    let first = ids(&v).into_iter().find(|x| x.1 == 12.5).unwrap().0;
    accept(&e, &first, "k1", false).await;
    let before = format!("{:?}", e.store.proposals("COAL", None, None).unwrap());
    e.store.rebuild().unwrap();
    let after = format!("{:?}", e.store.proposals("COAL", None, None).unwrap());
    assert_eq!(before, after);
    assert!(after.contains("superseded"));
}

#[test]
fn migration_010_keeps_the_rows_of_a_v9_store_and_allows_superseded() {
    use investing_app::research::store::Store;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("research.sqlite");
    let old = Store::open_at_version(&db, 9).unwrap();
    assert_eq!(old.user_version().unwrap(), 9);
    old.add_document(&investing_app::research::store::DocumentRow {
        id: "d".into(),
        ticker: "COAL".into(),
        kind: "laporan".into(),
        url: None,
        text: Some("t".into()),
        doc_date: None,
        added_at: "t".into(),
    })
    .unwrap();
    old.with_conn(|c| {
        c.execute(
            "INSERT INTO extraction_proposals (id, document_id, ticker, kpi_key, value, unit, page, snippet, rank, status, created_at)
             VALUES ('p-old', 'd', 'COAL', 'production_mt:2026Q2', 12.5, 'Mt', 1, 's', 1, 'rejected', 't')",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    drop(old);
    let store = Store::open(&db).unwrap();
    assert_eq!(
        store.user_version().unwrap(),
        investing_app::research::store::latest_version()
    );
    assert_eq!(store.proposal("p-old").unwrap().unwrap().status, "rejected");
    store
        .with_conn(|c| {
            c.execute(
                "INSERT INTO extraction_proposals (id, document_id, ticker, kpi_key, value, unit, page, snippet, rank, status, created_at)
                 VALUES ('p-new', 'd', 'COAL', 'production_mt:2026Q2', 13.0, 'Mt', 1, 's', 2, 'superseded', 't')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
}
