//! Writes the fictional demo PDFs used for README screenshots and manual testing.
//!   cargo run --example make_sample -- <output-folder>
//! All names, companies and amounts are invented.
use penadf::ops::{self, PagePoint, PageRect, create, markup};

const CONTRACT: &str = "PERJANJIAN SEWA RUKO\n\
Nomor: 014/SR/X/2026\n\n\
Pada hari ini, Senin 19 Oktober 2026, yang bertanda tangan di bawah ini:\n\n\
1. Ibu Ratna Wulandari, pemilik ruko, selanjutnya disebut PIHAK PERTAMA.\n\
2. Bapak Dimas Prakoso, CV Sinar Jaya Abadi, selanjutnya disebut PIHAK KEDUA.\n\n\
Pasal 1  Objek Sewa\n\
Ruko dua lantai di Jl. Melati Raya No. 12, Bandung.\n\n\
Pasal 2  Jangka Waktu\n\
Sewa berlaku 2 (dua) tahun, mulai 1 November 2026.\n\n\
Pasal 3  Harga Sewa\n\
Harga sewa Rp 96.000.000 per tahun, dibayar di muka.\n\n\
Pasal 4  Pemeliharaan\n\
PIHAK KEDUA menjaga kebersihan dan merawat bangunan.\n\n\n\
PIHAK PERTAMA                    PIHAK KEDUA\n\n\n\n\
Ratna Wulandari                  Dimas Prakoso\n";

/// A fictional handwritten signature in the app's stroke format (unit box, y up).
const SIGNATURE: &str = include_str!("sample-signature.txt");

fn main() -> anyhow::Result<()> {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&dir)?;

    // Contract, page 2 is an annex.
    let mut text = CONTRACT.to_string();
    text.push_str(&"\n".repeat(46));
    text.push_str("LAMPIRAN A  Denah Ruko\n\nLantai 1: area usaha 48 m2.\nLantai 2: gudang dan kantor 40 m2.\n");
    let pdf = create::from_text(&text)?;
    // Highlight the price clause and leave a note, as a reviewer would.
    let hit = ops::search(&pdf, None, "Harga sewa Rp 96.000.000")?.remove(0);
    let line = hit.rects[0];
    let area = PageRect {
        left: line.left - 2.0,
        bottom: line.bottom - 2.0,
        right: 560.0,
        top: line.top + 2.0,
    };
    let lines = markup::text_lines_in(&pdf, None, 0, area)?;
    let pdf = markup::add_text_markup(
        &pdf,
        None,
        0,
        markup::TextMarkup::Highlight,
        &lines,
        markup::Rgba::YELLOW,
    )?;
    let pdf = markup::add_note(
        &pdf,
        None,
        0,
        PagePoint {
            x: 470.0,
            y: line.top + 18.0,
        },
        "Cek lagi: sudah termasuk PPN?",
    )?;
    // The second party's signature above his name (fictional person, drawn signature).
    let name = ops::search(&pdf, None, "Dimas Prakoso")?
        .pop()
        .expect("name on page")
        .rects[0];
    let sig = markup::Signature::from_text(SIGNATURE);
    let rect = PageRect {
        left: name.left - 6.0,
        bottom: name.top + 4.0,
        right: name.left + 120.0,
        top: name.top + 46.0,
    };
    let pdf = markup::sign(&pdf, None, 0, rect, &sig)?;
    std::fs::write(dir.join("Perjanjian-Sewa-Ruko.pdf"), &pdf)?;

    // A second version with one changed clause, for Compare.
    let v2 =
        create::from_text(&text.replace("Rp 96.000.000 per tahun", "Rp 102.000.000 per tahun"))?;
    std::fs::write(dir.join("Perjanjian-Sewa-Ruko-v2.pdf"), v2)?;

    // Two more short documents so the tab bar has something to show.
    let invoice = create::from_text(
        "INVOICE INV-2026-0187\n\nCV Sinar Jaya Abadi\nJl. Merdeka No. 5, Bandung\n\nJasa desain interior ruko      Rp 18.500.000\nPPN 11%                        Rp  2.035.000\nTotal                          Rp 20.535.000\n\nJatuh tempo: 2 November 2026\n",
    )?;
    std::fs::write(dir.join("Invoice-Oktober.pdf"), invoice)?;
    let form = create::from_text(
        "FORMULIR CUTI KARYAWAN\n\nNama        : Dimas Prakoso\nDivisi      : Operasional\nTanggal     : 26 - 30 Oktober 2026\nAlasan      : Keperluan keluarga\n\nDisetujui oleh atasan:\n",
    )?;
    std::fs::write(dir.join("Form-Cuti.pdf"), form)?;
    println!("wrote samples to {}", dir.display());
    Ok(())
}
