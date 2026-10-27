//! R8 document extraction at API level (research plan §6 R8; prd.md A8.1 to A8.3): upload a
//! synthetic PDF or PPTX to a dossier, read the rule-based proposals with their pages,
//! accept (writes the KPI with the document as provenance) or reject (stores nothing), and
//! `rebuild` restores it all from the log. Every file is generated here; nothing is real.

#[path = "support/pack_fixture.rs"]
mod pack_fixture;
#[path = "support/r4_env.rs"]
mod r4_env;
#[path = "support/xbrl_fixture.rs"]
mod xbrl_fixture;

use std::io::Write;

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use http_body_util::BodyExt;
use r4_env::{env, Env};
use serde_json::{json, Value};
use tower::ServiceExt;

const BOUNDARY: &str = "----r8-test-boundary";

// ---- synthetic documents ----

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// A minimal PDF with one text line per entry, Helvetica, one page per slice.
fn pdf(pages: &[&[&str]]) -> Vec<u8> {
    let n = pages.len();
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    let mut objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
    ];
    for (i, lines) in pages.iter().enumerate() {
        objs.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            5 + 2 * i
        ));
        let mut c = String::from("BT /F1 12 Tf 72 720 Td 18 TL\n");
        for l in *lines {
            c.push_str(&format!("({}) Tj T*\n", esc(l)));
        }
        c.push_str("ET");
        objs.push(format!("<< /Length {} >>\nstream\n{c}\nendstream", c.len()));
    }
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objs.len() + 1).as_bytes());
    out.extend_from_slice(format!("{:010} 65535 f \n", 0).as_bytes());
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

struct Slide<'a> {
    paras: &'a [&'a str],
    table: &'a [&'a [&'a str]],
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}

/// A minimal PPTX. `slides[k]` is the part `ppt/slides/slide{k+1}.xml`; `order` lists the
/// parts in presentation order (indexes into `slides`).
fn pptx(slides: &[Slide<'_>], order: &[usize]) -> Vec<u8> {
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
        let ids: String = order
            .iter()
            .enumerate()
            .map(|(pos, k)| format!(r#"<p:sldId id="{}" r:id="rId{}"/>"#, 256 + pos, k + 2))
            .collect();
        put(
            "ppt/presentation.xml",
            format!(
                r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst>{ids}</p:sldIdLst></p:presentation>"#
            ),
        );
        let rels: String = (0..slides.len())
            .map(|k| {
                format!(
                    r#"<Relationship Id="rId{}" Type="slide" Target="slides/slide{}.xml"/>"#,
                    k + 2,
                    k + 1
                )
            })
            .collect();
        put(
            "ppt/_rels/presentation.xml.rels",
            format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="slideMaster" Target="slideMasters/slideMaster1.xml"/>{rels}</Relationships>"#
            ),
        );
        for (k, s) in slides.iter().enumerate() {
            let paras: String = s
                .paras
                .iter()
                .map(|t| format!("<a:p><a:r><a:t>{}</a:t></a:r></a:p>", xml(t)))
                .collect();
            let rows: String = s
                .table
                .iter()
                .map(|row| {
                    let cells: String = row
                        .iter()
                        .map(|c| {
                            format!(
                                "<a:tc><a:txBody><a:p><a:r><a:t>{}</a:t></a:r></a:p></a:txBody></a:tc>",
                                xml(c)
                            )
                        })
                        .collect();
                    format!("<a:tr>{cells}</a:tr>")
                })
                .collect();
            put(
                &format!("ppt/slides/slide{}.xml", k + 1),
                format!(
                    r#"<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody>{paras}</p:txBody></p:sp><p:graphicFrame><a:graphic><a:graphicData><a:tbl>{rows}</a:tbl></a:graphicData></a:graphic></p:graphicFrame></p:spTree></p:cSld></p:sld>"#
                ),
            );
        }
        zip.finish().unwrap();
    }
    buf.into_inner()
}

/// COAL's first quarter-two report, as a PDF: page 1 volumes, page 2 prices.
fn coal_pdf() -> Vec<u8> {
    pdf(&[
        &[
            "Kinerja operasional Q2 2026",
            "Produksi 12,5 juta ton",
            "Penjualan 11.200 kt",
        ],
        &[
            "Average selling price USD 65,3/t",
            "Cash cost 41,0 USD/t",
            "CAR 23,4%",
            "Coal reserves 120 Mt",
        ],
    ])
}

fn coal_pptx() -> Vec<u8> {
    pptx(
        &[
            Slide {
                paras: &["Harga dan biaya"],
                table: &[
                    &["Average selling price", "USD 65,3/t"],
                    &["Cash cost", "41,0 USD/t"],
                ],
            },
            Slide {
                paras: &["Kinerja", "Produksi 12,5 Mt"],
                table: &[&["Penjualan", "11,0 Mt"]],
            },
        ],
        // The slide stored second comes first in the presentation.
        &[1, 0],
    )
}

// ---- requests ----

fn multipart(fields: &[(&str, &str)], file: Option<(&str, &[u8])>) -> Vec<u8> {
    let mut body = Vec::new();
    for (k, v) in fields {
        body.extend_from_slice(
            format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n")
                .as_bytes(),
        );
    }
    if let Some((name, bytes)) = file {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

const FORM: [(&str, &str); 4] = [
    ("kind", "presentasi"),
    ("doc_date", "2026-08-20"),
    ("fy", "2026"),
    ("q", "2"),
];

async fn raw(
    e: &Env,
    method: Method,
    uri: &str,
    key: Option<&str>,
    content_type: &str,
    body: Vec<u8>,
) -> (StatusCode, Value, HeaderMap) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", content_type);
    if let Some(k) = key {
        req = req.header("idempotency-key", k);
    }
    let res = e
        .app
        .clone()
        .oneshot(req.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        headers,
    )
}

async fn upload_with(
    e: &Env,
    ticker: &str,
    key: Option<&str>,
    fields: &[(&str, &str)],
    file: Option<(&str, &[u8])>,
) -> (StatusCode, Value, HeaderMap) {
    raw(
        e,
        Method::POST,
        &format!("/api/research/{ticker}/documents/upload"),
        key,
        &format!("multipart/form-data; boundary={BOUNDARY}"),
        multipart(fields, file),
    )
    .await
}

async fn upload(e: &Env, ticker: &str, key: &str, name: &str, bytes: &[u8]) -> Value {
    let (s, v, _) = upload_with(e, ticker, Some(key), &FORM, Some((name, bytes))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    v
}

fn proposal<'a>(v: &'a Value, metric: &str) -> &'a Value {
    v["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["metric"] == metric && p["rank"] == 1)
        .unwrap_or_else(|| panic!("no rank-1 proposal for {metric}: {v}"))
}

fn files_in(e: &Env) -> Vec<String> {
    let dir = e.dir.path().join("research/documents");
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = rd
        .map(|f| f.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

// ---- upload and proposals (A8.1) ----

#[tokio::test]
async fn a_pdf_upload_stores_the_file_files_the_document_and_proposes_with_pages() {
    let e = env();
    let bytes = coal_pdf();
    let v = upload(&e, "COAL", "u1", "paparan-q2.pdf", &bytes).await;

    assert_eq!(v["created"], true);
    assert_eq!(v["upload"]["format"], "pdf");
    assert_eq!(v["upload"]["locator"], "page");
    assert_eq!(v["upload"]["pages"], 2);
    assert_eq!(v["upload"]["pages_with_text"], 2);
    assert_eq!(v["upload"]["bytes"], bytes.len());
    assert_eq!(v["period"], json!({"fy": 2026, "q": 2, "label": "2026Q2"}));
    assert_eq!(v["pack"]["id"], "coal_mining");

    // The file is kept by its content hash.
    let sha = v["upload"]["sha256"].as_str().unwrap();
    assert_eq!(sha.len(), 64);
    assert_eq!(files_in(&e), [format!("{sha}.pdf")]);
    let stored = std::fs::read(e.dir.path().join(format!("research/documents/{sha}.pdf"))).unwrap();
    assert_eq!(stored, bytes);

    // The document is filed through the ordinary documents path.
    let doc = &v["document"];
    assert_eq!(doc["kind"], "presentasi");
    assert_eq!(doc["doc_date"], "2026-08-20");
    assert!(doc["text"]
        .as_str()
        .unwrap()
        .starts_with("Unggahan PDF: paparan-q2.pdf"));
    let (s, list) = e.get("/api/research/COAL/documents").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list["items"][0]["id"], doc["id"]);

    // Proposals with their pages, in the KPI's unit.
    let production = proposal(&v, "production_mt");
    assert_eq!(production["value"], 12.5);
    assert_eq!(production["unit"], "Mt");
    assert_eq!(production["page"], 1);
    assert_eq!(production["kpi_key"], "production_mt:2026Q2");
    assert_eq!(production["locator"], "page");
    assert_eq!(production["status"], "pending");
    assert_eq!(production["document_id"], doc["id"]);
    assert!(production["snippet"]
        .as_str()
        .unwrap()
        .contains("Produksi 12,5 juta ton"));
    assert!(production["snippet"].as_str().unwrap().chars().count() <= 160);
    let sales = proposal(&v, "sales_mt");
    assert_eq!(
        (sales["value"].as_f64().unwrap() * 1e6).round(),
        11_200_000.0
    );
    assert_eq!(sales["page"], 1);
    let asp = proposal(&v, "asp_usd_t");
    assert_eq!(
        (asp["value"].as_f64().unwrap(), asp["page"].clone()),
        (65.3, json!(2))
    );
    assert_eq!(asp["unit"], "USD/t");
    assert_eq!(proposal(&v, "cash_cost_usd_t")["value"], 41.0);
    // CAR is a bank KPI, the annual reserves wait for a Q4 report: no match, no proposal.
    let metrics: Vec<&str> = v["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["metric"].as_str().unwrap())
        .collect();
    assert!(
        !metrics.contains(&"car") && !metrics.contains(&"reserves_mt"),
        "{metrics:?}"
    );
    assert_eq!(
        v["counts"],
        json!({"pending": 4, "accepted": 0, "rejected": 0, "superseded": 0})
    );

    // Nothing reaches kpi_values without an accept.
    assert!(e.store.kpis("COAL").unwrap().is_empty());
    assert!(e.store.evidence("COAL").unwrap().is_empty());
}

#[tokio::test]
async fn a_pptx_upload_reads_slides_in_presentation_order_and_table_rows_as_lines() {
    let e = env();
    let v = upload(&e, "COAL", "u1", "paparan.pptx", &coal_pptx()).await;
    assert_eq!(v["upload"]["format"], "pptx");
    assert_eq!(v["upload"]["locator"], "slide");
    assert_eq!(v["upload"]["pages"], 2);
    assert!(v["document"]["text"]
        .as_str()
        .unwrap()
        .starts_with("Unggahan PPTX: paparan.pptx"));
    let sha = v["upload"]["sha256"].as_str().unwrap();
    assert_eq!(files_in(&e), [format!("{sha}.pptx")]);

    let production = proposal(&v, "production_mt");
    assert_eq!(
        (production["value"].clone(), production["page"].clone()),
        (json!(12.5), json!(1))
    );
    assert_eq!(production["locator"], "slide");
    // A table row (label cell and figure cell) is one line.
    let sales = proposal(&v, "sales_mt");
    assert_eq!(
        (sales["value"].clone(), sales["page"].clone()),
        (json!(11.0), json!(1))
    );
    let asp = proposal(&v, "asp_usd_t");
    assert_eq!(
        (asp["value"].clone(), asp["page"].clone()),
        (json!(65.3), json!(2))
    );
    assert_eq!(proposal(&v, "cash_cost_usd_t")["page"], 2);
    assert!(e.store.kpis("COAL").unwrap().is_empty());
}

#[tokio::test]
async fn an_annual_kpi_is_proposed_for_a_q4_report_only() {
    let e = env();
    let bytes = coal_pdf();
    let fields = [
        ("kind", "laporan"),
        ("doc_date", "2026-02-20"),
        ("fy", "2025"),
        ("q", "4"),
    ];
    let (s, v, _) = upload_with(&e, "COAL", Some("u4"), &fields, Some(("ar.pdf", &bytes))).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let reserves = proposal(&v, "reserves_mt");
    assert_eq!(reserves["value"], 120.0);
    assert_eq!(reserves["kpi_key"], "reserves_mt:2025Q4");
    assert_eq!(reserves["page"], 2);
}

#[tokio::test]
async fn a_ticker_without_a_pack_files_the_document_and_proposes_nothing() {
    let e = env();
    let v = upload(&e, "NONE", "u1", "x.pdf", &coal_pdf()).await;
    assert!(v["pack"].is_null());
    assert_eq!(v["proposals"], json!([]));
    assert_eq!(e.store.documents("NONE").unwrap().len(), 1);
}

#[tokio::test]
async fn the_same_file_is_stored_once_and_a_retry_changes_nothing() {
    let e = env();
    let bytes = coal_pdf();
    let first = upload(&e, "COAL", "u1", "a.pdf", &bytes).await;
    // The same key and request replays the stored response.
    let (s, again, headers) =
        upload_with(&e, "COAL", Some("u1"), &FORM, Some(("a.pdf", &bytes))).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(headers["idempotent-replay"], "true");
    assert_eq!(again, first);
    // A new key with the same file and period files nothing new.
    let (s, second, _) =
        upload_with(&e, "COAL", Some("u2"), &FORM, Some(("renamed.pdf", &bytes))).await;
    assert_eq!(s, StatusCode::OK, "{second}");
    assert_eq!(second["created"], false);
    assert_eq!(second["document"]["id"], first["document"]["id"]);
    assert_eq!(second["proposals"], first["proposals"]);
    assert_eq!(e.store.documents("COAL").unwrap().len(), 1);
    assert_eq!(e.store.proposals("COAL", None, None).unwrap().len(), 4);
    assert_eq!(files_in(&e).len(), 1);
    // The same bytes for another period are another document over the same file.
    let fields = [
        ("kind", "presentasi"),
        ("doc_date", "2026-08-20"),
        ("fy", "2026"),
        ("q", "3"),
    ];
    let (s, third, _) = upload_with(&e, "COAL", Some("u3"), &fields, Some(("a.pdf", &bytes))).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_ne!(third["document"]["id"], first["document"]["id"]);
    assert_eq!(files_in(&e).len(), 1);
    assert_eq!(
        proposal(&third, "production_mt")["kpi_key"],
        "production_mt:2026Q3"
    );
    // A decided proposal is not reopened by a retry of its upload.
    let id = proposal(&first, "production_mt")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (s, _) = e
        .post(
            &format!("/api/research/proposals/{id}/reject"),
            "r1",
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, fourth, _) = upload_with(&e, "COAL", Some("u4"), &FORM, Some(("a.pdf", &bytes))).await;
    assert_eq!(proposal(&fourth, "production_mt")["status"], "rejected");
}

#[tokio::test]
async fn extraction_is_deterministic() {
    let e = env();
    let a = upload(&e, "COAL", "u1", "a.pdf", &coal_pdf()).await;
    let fields = [
        ("kind", "presentasi"),
        ("doc_date", "2026-08-20"),
        ("fy", "2026"),
        ("q", "3"),
    ];
    let bytes = coal_pdf();
    let (_, b, _) = upload_with(&e, "COAL", Some("u2"), &fields, Some(("a.pdf", &bytes))).await;
    let strip = |v: &Value| -> Vec<(String, Value, Value, Value, Value)> {
        v["proposals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p["metric"].as_str().unwrap().to_string(),
                    p["value"].clone(),
                    p["page"].clone(),
                    p["snippet"].clone(),
                    p["rank"].clone(),
                )
            })
            .collect()
    };
    assert_eq!(strip(&a), strip(&b));
}

// ---- validation ----

#[tokio::test]
async fn bad_uploads_are_refused_with_the_field_named_and_nothing_is_stored() {
    let e = env();
    let pdf_bytes = coal_pdf();
    let url = "/api/research/COAL/documents/upload";
    let ct = format!("multipart/form-data; boundary={BOUNDARY}");

    // No Idempotency-Key.
    let (s, v, _) = upload_with(&e, "COAL", None, &FORM, Some(("a.pdf", &pdf_bytes))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v["detail"][0]["loc"], json!(["header", "idempotency-key"]));

    // Form fields.
    for (field, bad) in [
        ("kind", "berita"),
        ("doc_date", "2026-13-01"),
        ("doc_date", "2026-10-26"),
        ("fy", "26"),
        ("q", "5"),
    ] {
        let fields: Vec<(&str, &str)> = FORM
            .iter()
            .map(|(k, v)| if *k == field { (*k, bad) } else { (*k, *v) })
            .collect();
        let (s, v, _) =
            upload_with(&e, "COAL", Some("k"), &fields, Some(("a.pdf", &pdf_bytes))).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{field}={bad}");
        assert_eq!(
            v["detail"][0]["loc"],
            json!(["body", field]),
            "{field}={bad}"
        );
    }
    let no_date: Vec<(&str, &str)> = FORM
        .iter()
        .filter(|(k, _)| *k != "doc_date")
        .copied()
        .collect();
    let (s, v, _) = upload_with(&e, "COAL", Some("k"), &no_date, Some(("a.pdf", &pdf_bytes))).await;
    assert_eq!(
        (s, &v["detail"][0]["loc"]),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            &json!(["body", "doc_date"])
        )
    );

    // The file.
    let (s, v, _) = upload_with(&e, "COAL", Some("k"), &FORM, None).await;
    assert_eq!(
        (s, &v["detail"][0]["loc"]),
        (StatusCode::UNPROCESSABLE_ENTITY, &json!(["body", "file"]))
    );
    for (name, bytes) in [
        // The magic bytes decide, not the name.
        ("a.pdf", b"just some text, not a pdf".to_vec()),
        ("a.pdf", b"PK\x03\x04 not a zip at all".to_vec()),
        ("a.exe", b"MZ\x90\x00 an executable".to_vec()),
    ] {
        let (s, v, _) = upload_with(&e, "COAL", Some("k"), &FORM, Some((name, &bytes))).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{name}");
        assert_eq!(v["detail"][0]["loc"], json!(["body", "file"]));
        assert_eq!(v["detail"][0]["type"], "file_type");
    }
    // A ZIP that is not a presentation (a Word-like package).
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"<w:document/>").unwrap();
        zip.finish().unwrap();
    }
    let (s, v, _) = upload_with(
        &e,
        "COAL",
        Some("k"),
        &FORM,
        Some(("a.pptx", &buf.into_inner())),
    )
    .await;
    assert_eq!(
        (s, &v["detail"][0]["type"]),
        (StatusCode::UNPROCESSABLE_ENTITY, &json!("file_type"))
    );
    // Right magic bytes, unreadable inside.
    let (s, v, _) = upload_with(
        &e,
        "COAL",
        Some("k"),
        &FORM,
        Some(("a.pdf", b"%PDF-1.4 cut off")),
    )
    .await;
    assert_eq!(
        (s, &v["detail"][0]["type"]),
        (StatusCode::UNPROCESSABLE_ENTITY, &json!("file_unreadable"))
    );

    // Not multipart.
    let (s, v, _) = raw(
        &e,
        Method::POST,
        url,
        Some("k"),
        "application/json",
        b"{}".to_vec(),
    )
    .await;
    assert_eq!(
        (s, &v["detail"][0]["type"]),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            &json!("multipart_invalid")
        )
    );
    let (s, _, _) = raw(&e, Method::POST, url, Some("k"), &ct, b"garbage".to_vec()).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // Unknown ticker.
    let (s, _, _) = upload_with(&e, "ZZZZ", Some("k"), &FORM, Some(("a.pdf", &pdf_bytes))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    assert!(e.store.documents("COAL").unwrap().is_empty());
    assert!(e.store.proposals("COAL", None, None).unwrap().is_empty());
    assert!(files_in(&e).is_empty());
}

#[tokio::test]
async fn a_file_over_twenty_megabytes_is_refused() {
    let e = env();
    let mut big = b"%PDF-1.4\n".to_vec();
    big.resize(20 * 1024 * 1024 + 1, b' ');
    let (s, _, _) = upload_with(&e, "COAL", Some("k"), &FORM, Some(("big.pdf", &big))).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    let mut ok = b"%PDF-1.4\n".to_vec();
    ok.resize(20 * 1024 * 1024, b' ');
    let (s, v, _) = upload_with(&e, "COAL", Some("k2"), &FORM, Some(("edge.pdf", &ok))).await;
    // Exactly 20 MB passes the size gate; the padding is no readable PDF.
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    assert_eq!(v["detail"][0]["type"], "file_unreadable");
    assert!(files_in(&e).is_empty());
}

// ---- list, accept, reject (A8.2) ----

#[tokio::test]
async fn proposals_are_listed_by_document_and_status() {
    let e = env();
    let a = upload(&e, "COAL", "u1", "a.pdf", &coal_pdf()).await;
    let b = upload(&e, "COAL", "u2", "b.pptx", &coal_pptx()).await;
    let (s, all) = e.get("/api/research/COAL/proposals").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(all["items"].as_array().unwrap().len(), 8);
    assert_eq!(all["counts"]["pending"], 8);
    assert_eq!(all["documents"].as_array().unwrap().len(), 2);
    let doc_a = a["document"]["id"].as_str().unwrap();
    let (_, only_a) = e
        .get(&format!("/api/research/COAL/proposals?document_id={doc_a}"))
        .await;
    let items = only_a["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    assert!(items.iter().all(|p| p["document_id"] == doc_a));
    assert_eq!(only_a["document_id"], doc_a);
    assert!(items[0]["metric_label"].is_string());
    // Reject one, then filter by status.
    let id = proposal(&b, "sales_mt")["id"].as_str().unwrap().to_string();
    e.post(
        &format!("/api/research/proposals/{id}/reject"),
        "r1",
        json!({}),
    )
    .await;
    let (_, rejected) = e.get("/api/research/COAL/proposals?status=rejected").await;
    assert_eq!(rejected["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        rejected["counts"],
        json!({"pending": 7, "accepted": 0, "rejected": 1, "superseded": 0})
    );
    let (s, v) = e.get("/api/research/COAL/proposals?status=maybe").await;
    assert_eq!(
        (s, &v["detail"][0]["loc"]),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            &json!(["query", "status"])
        )
    );
    assert_eq!(
        e.get("/api/research/BANK/proposals").await.1["items"],
        json!([])
    );
    assert_eq!(
        e.get("/api/research/ZZZZ/proposals").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn accepting_writes_the_kpi_with_the_document_as_provenance_and_an_evidence_link() {
    let e = env();
    let v = upload(&e, "COAL", "u1", "a.pdf", &coal_pdf()).await;
    let doc_id = v["document"]["id"].as_str().unwrap();
    let p = proposal(&v, "asp_usd_t");
    let id = p["id"].as_str().unwrap();
    let uri = format!("/api/research/proposals/{id}/accept");

    // Without a key: 422, nothing written.
    let (s, _, _) = raw(&e, Method::POST, &uri, None, "application/json", Vec::new()).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(e.store.kpis("COAL").unwrap().is_empty());

    let (s, done, _) = raw(
        &e,
        Method::POST,
        &uri,
        Some("a1"),
        "application/json",
        Vec::new(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{done}");
    assert_eq!(done["proposal"]["status"], "accepted");
    assert_eq!(done["proposal"]["decided_at"], "2026-10-24T09:00:00+00:00");
    assert_eq!(done["replaced"], false);
    assert_eq!(done["kpi"]["value"], 65.3);
    assert_eq!(done["kpi"]["unit"], "USD/t");
    assert_eq!(
        (done["kpi"]["fy"].clone(), done["kpi"]["q"].clone()),
        (json!(2026), json!(2))
    );
    assert_eq!(done["kpi"]["provenance"]["document_date"], "2026-08-20");
    let reference = done["kpi"]["provenance"]["document"].as_str().unwrap();
    assert!(
        reference.contains("Presentasi")
            && reference.contains("PDF")
            && reference.contains("hlm 2"),
        "{reference}"
    );
    assert_eq!(
        done["evidence"],
        json!({"document_id": doc_id, "kpi_key": "asp_usd_t:2026Q2"})
    );

    // The store holds the value, and the pack section shows it entered.
    let kpi = e.store.kpi("COAL", "asp_usd_t", 2026, 2).unwrap().unwrap();
    assert_eq!(
        (kpi.value, kpi.unknown, kpi.unit.as_str()),
        (Some(65.3), false, "USD/t")
    );
    let links = e.store.evidence("COAL").unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].kpi_key.as_deref(), Some("asp_usd_t:2026Q2"));
    let (_, docs) = e.get("/api/research/COAL/documents").await;
    assert_eq!(
        docs["items"][0]["evidence"],
        json!([{"claim_id": null, "kpi_key": "asp_usd_t:2026Q2"}])
    );

    // The same key replays; another key finds it decided.
    let (s, again, headers) = raw(
        &e,
        Method::POST,
        &uri,
        Some("a1"),
        "application/json",
        Vec::new(),
    )
    .await;
    assert_eq!(
        (s, headers["idempotent-replay"].to_str().unwrap()),
        (StatusCode::OK, "true")
    );
    assert_eq!(again, done);
    let (s, v2, _) = raw(
        &e,
        Method::POST,
        &uri,
        Some("a2"),
        "application/json",
        Vec::new(),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert!(v2["detail"].as_str().unwrap().contains("already accepted"));
    // Only that one cell was written; its siblings are still pending.
    assert_eq!(e.store.kpis("COAL").unwrap().len(), 1);
    let (_, list) = e.get("/api/research/COAL/proposals").await;
    assert_eq!(
        list["counts"],
        json!({"pending": 3, "accepted": 1, "rejected": 0, "superseded": 0})
    );
    assert_eq!(
        e.post("/api/research/proposals/p-none/accept", "a3", json!({}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn accepting_over_an_owner_value_is_a_409_unless_replace_is_sent() {
    let e = env();
    let v = upload(&e, "COAL", "u1", "a.pdf", &coal_pdf()).await;
    let id = proposal(&v, "production_mt")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let uri = format!("/api/research/proposals/{id}/accept");
    // The owner typed 9.9 for that cell.
    let (s, _) = e
        .send(
            Method::PUT,
            "/api/research/COAL/kpi/production_mt/2026/2",
            "k1",
            json!({"value": 9.9, "document": "Catatan saya", "document_date": "2026-07-30"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);

    let (s, v409) = e.post(&uri, "a1", json!({})).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let detail = v409["detail"].as_str().unwrap();
    assert!(
        detail.contains("9.9") && detail.contains("replace"),
        "{detail}"
    );
    let kept = e
        .store
        .kpi("COAL", "production_mt", 2026, 2)
        .unwrap()
        .unwrap();
    assert_eq!(
        (kept.value, kept.document.as_str()),
        (Some(9.9), "Catatan saya")
    );
    assert_eq!(e.store.proposal(&id).unwrap().unwrap().status, "pending");
    assert!(e.store.evidence("COAL").unwrap().is_empty());
    // `replace` must be a boolean.
    let (s, bad) = e.post(&uri, "a2", json!({"replace": "yes"})).await;
    assert_eq!(
        (s, &bad["detail"][0]["loc"]),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            &json!(["body", "replace"])
        )
    );
    // An explicit unknown is an owner answer too.
    let (s, done) = e.post(&uri, "a3", json!({"replace": true})).await;
    assert_eq!(s, StatusCode::OK, "{done}");
    assert_eq!(done["replaced"], true);
    let now = e
        .store
        .kpi("COAL", "production_mt", 2026, 2)
        .unwrap()
        .unwrap();
    assert_eq!(now.value, Some(12.5));
    assert!(now.document.contains("hlm 1"));
    assert_eq!(e.store.proposal(&id).unwrap().unwrap().status, "accepted");

    let unknown_id = proposal(&v, "cash_cost_usd_t")["id"]
        .as_str()
        .unwrap()
        .to_string();
    e.send(
        Method::PUT,
        "/api/research/COAL/kpi/cash_cost_usd_t/2026/2",
        "k2",
        json!({"unknown": true, "document": "Tidak dilaporkan", "document_date": "2026-07-30"}),
    )
    .await;
    let (s, v409) = e
        .post(
            &format!("/api/research/proposals/{unknown_id}/accept"),
            "a4",
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert!(v409["detail"].as_str().unwrap().contains("unknown"));
}

#[tokio::test]
async fn accepting_an_annual_kpi_stores_it_at_q4() {
    let e = env();
    let bytes = coal_pdf();
    let fields = [
        ("kind", "laporan"),
        ("doc_date", "2026-02-20"),
        ("fy", "2025"),
        ("q", "4"),
    ];
    let (_, v, _) = upload_with(&e, "COAL", Some("u4"), &fields, Some(("ar.pdf", &bytes))).await;
    let id = proposal(&v, "reserves_mt")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (s, done) = e
        .post(
            &format!("/api/research/proposals/{id}/accept"),
            "a1",
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{done}");
    let kpi = e
        .store
        .kpi("COAL", "reserves_mt", 2025, 4)
        .unwrap()
        .unwrap();
    assert_eq!((kpi.value, kpi.unit.as_str()), (Some(120.0), "Mt"));
}

#[tokio::test]
async fn accept_is_refused_when_the_kpi_left_the_pack_in_effect() {
    let e = env();
    let v = upload(&e, "COAL", "u1", "a.pdf", &coal_pdf()).await;
    let id = proposal(&v, "production_mt")["id"]
        .as_str()
        .unwrap()
        .to_string();
    // The owner switches COAL to the bank pack; production_mt is no longer one of its KPIs.
    let (s, _) = e
        .send(
            Method::PUT,
            "/api/research/universe/COAL",
            "o1",
            json!({"pack_override": "bank_sharia"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, v409) = e
        .post(
            &format!("/api/research/proposals/{id}/accept"),
            "a1",
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert!(v409["detail"]
        .as_str()
        .unwrap()
        .contains("no longer in the pack"));
    assert!(e.store.kpis("COAL").unwrap().is_empty());
    // It can still be rejected.
    let (s, _) = e
        .post(
            &format!("/api/research/proposals/{id}/reject"),
            "r1",
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn rejecting_stores_nothing_and_is_final() {
    let e = env();
    let v = upload(&e, "COAL", "u1", "a.pdf", &coal_pdf()).await;
    let id = proposal(&v, "sales_mt")["id"].as_str().unwrap().to_string();
    let reject = format!("/api/research/proposals/{id}/reject");
    let (s, _, _) = raw(
        &e,
        Method::POST,
        &reject,
        None,
        "application/json",
        Vec::new(),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, done) = e.post(&reject, "r1", json!({})).await;
    assert_eq!(s, StatusCode::OK, "{done}");
    assert_eq!(done["proposal"]["status"], "rejected");
    assert_eq!(done["proposal"]["decided_at"], "2026-10-24T09:00:00+00:00");
    assert!(e.store.kpis("COAL").unwrap().is_empty());
    assert!(e.store.evidence("COAL").unwrap().is_empty());
    // Decided is decided, either way.
    assert_eq!(
        e.post(&reject, "r2", json!({})).await.0,
        StatusCode::CONFLICT
    );
    let (s, v409) = e
        .post(
            &format!("/api/research/proposals/{id}/accept"),
            "a1",
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert!(v409["detail"]
        .as_str()
        .unwrap()
        .contains("already rejected"));
    assert!(e.store.kpis("COAL").unwrap().is_empty());
    assert_eq!(
        e.post("/api/research/proposals/p-none/reject", "r3", json!({}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn an_accepted_value_re_evaluates_a_claim_on_that_kpi() {
    let e = env();
    let (s, c) = e
        .post(
            "/api/research/COAL/claims",
            "c1",
            json!({
                "statement": "Produksi tidak di bawah sepuluh", "metric": "coal_mining.production_mt",
                "comparator": ">=", "threshold": 10, "window": "latest", "due": "2026-12-31",
            }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{c}");
    assert_eq!(c["status"], "not_testable");
    let v = upload(&e, "COAL", "u1", "a.pdf", &coal_pdf()).await;
    let id = proposal(&v, "production_mt")["id"]
        .as_str()
        .unwrap()
        .to_string();
    e.post(
        &format!("/api/research/proposals/{id}/accept"),
        "a1",
        json!({}),
    )
    .await;
    let (_, claims) = e.get("/api/research/COAL/claims").await;
    let status = claims["items"][0]["status"].as_str().unwrap().to_string();
    assert_eq!(status, "holding", "{claims}");
}

// ---- rebuild and migration ----

#[tokio::test]
async fn rebuild_restores_documents_proposals_decisions_and_accepted_values() {
    let e = env();
    let v = upload(&e, "COAL", "u1", "a.pdf", &coal_pdf()).await;
    let accept = proposal(&v, "production_mt")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let reject = proposal(&v, "sales_mt")["id"].as_str().unwrap().to_string();
    assert_eq!(
        e.post(
            &format!("/api/research/proposals/{accept}/accept"),
            "a1",
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        e.post(
            &format!("/api/research/proposals/{reject}/reject"),
            "r1",
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );

    let documents = e.store.documents("COAL").unwrap();
    let proposals = e.store.proposals("COAL", None, None).unwrap();
    let kpis = e.store.kpis("COAL").unwrap();
    let links = e.store.evidence("COAL").unwrap();
    assert_eq!(proposals.len(), 4);
    let status = |rows: &[finance_app::research::store::ProposalRow], id: &str| {
        rows.iter().find(|p| p.id == id).unwrap().status.clone()
    };
    assert_eq!(
        (
            status(&proposals, &accept).as_str(),
            status(&proposals, &reject).as_str()
        ),
        ("accepted", "rejected")
    );

    let replayed = e.store.rebuild().unwrap();
    assert!(replayed > 0);
    assert_eq!(e.store.documents("COAL").unwrap(), documents);
    assert_eq!(e.store.proposals("COAL", None, None).unwrap(), proposals);
    assert_eq!(e.store.kpis("COAL").unwrap(), kpis);
    assert_eq!(e.store.evidence("COAL").unwrap(), links);

    // A fresh database next to the same log rebuilds the same state.
    let fresh_dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        e.dir.path().join("research-log.jsonl"),
        fresh_dir.path().join("research-log.jsonl"),
    )
    .unwrap();
    let fresh =
        finance_app::research::store::Store::open(&fresh_dir.path().join("research.sqlite"))
            .unwrap();
    assert_eq!(fresh.proposals("COAL", None, None).unwrap(), proposals);
    assert_eq!(fresh.documents("COAL").unwrap(), documents);
}

#[test]
fn migrations_009_and_010_create_the_proposals_table_and_reopening_changes_nothing() {
    use finance_app::research::store::{latest_version, Store};
    assert!(latest_version() >= 10, "010 is user_version 10");
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("research.sqlite");
    let store = Store::open(&db).unwrap();
    assert_eq!(store.user_version().unwrap(), latest_version());
    assert!(store
        .table_names()
        .unwrap()
        .contains(&"extraction_proposals".to_string()));
    let columns: Vec<String> = store
        .with_conn(|c| {
            let mut st = c.prepare("SELECT name FROM pragma_table_info('extraction_proposals')")?;
            let rows = st.query_map([], |r| r.get::<_, String>(0))?;
            Ok(rows.collect::<Result<_, _>>()?)
        })
        .unwrap();
    assert_eq!(
        columns,
        [
            "id",
            "document_id",
            "ticker",
            "kpi_key",
            "value",
            "unit",
            "page",
            "snippet",
            "rank",
            "status",
            "created_at",
            "decided_at"
        ]
    );
    drop(store);
    assert_eq!(
        Store::open(&db).unwrap().user_version().unwrap(),
        latest_version()
    );
    // A store left at v8 by the previous release gets the table on the next open.
    let old = dir.path().join("old.sqlite");
    drop(Store::open_at_version(&old, 8).unwrap());
    let upgraded = Store::open(&old).unwrap();
    assert_eq!(upgraded.user_version().unwrap(), latest_version());
    assert!(upgraded
        .table_names()
        .unwrap()
        .contains(&"extraction_proposals".to_string()));
}

// ---- A8.3: no network, no model ----

#[test]
fn the_extraction_code_has_no_network_model_or_process_calls() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/research");
    let files = [
        root.join("extract/mod.rs"),
        root.join("extract/doc.rs"),
        root.join("extract/rules.rs"),
        root.join("api/r8.rs"),
        root.join("store/r8.rs"),
    ];
    let banned = [
        "reqwest",
        "http://",
        "https://",
        "TcpStream",
        "UdpSocket",
        "std::process",
        "Command::new",
        "openai",
        "anthropic",
        "llm",
        "SystemTime",
        "Utc::now",
        "rand::",
    ];
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        // The tests' own prose may name these; only code outside `#[cfg(test)]` counts.
        let code = text.split("#[cfg(test)]").next().unwrap();
        for word in banned {
            assert!(!code.contains(word), "{} mentions {word}", f.display());
        }
    }
}
