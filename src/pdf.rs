//! A minimal PDF writer: letter pages of lines, rectangles, curves and
//! Helvetica text, which is all a maze needs.

use std::fmt::Write as _;
use std::io::{self, Write};

pub const PAGE_W: f64 = 8.5 * 72.0;
pub const PAGE_H: f64 = 11.0 * 72.0;

/// Drawing commands for one page, in points from the top-left corner.
pub struct Canvas {
    buf: String,
}

impl Canvas {
    pub fn new() -> Canvas {
        // Flip the page so y runs down, as on screen.
        Canvas { buf: format!("1 0 0 -1 0 {PAGE_H} cm\n") }
    }

    fn nums(&mut self, vals: &[f64], op: &str) {
        for v in vals {
            let s = format!("{v:.2}");
            let s = s.trim_end_matches('0').trim_end_matches('.');
            self.buf.push_str(if s == "-0" { "0" } else { s });
            self.buf.push(' ');
        }
        self.buf.push_str(op);
        self.buf.push('\n');
    }

    pub fn colour(&mut self, c: [f64; 3]) {
        self.nums(&c, "rg");
        self.nums(&c, "RG");
    }
    pub fn width(&mut self, w: f64) {
        self.nums(&[w], "w");
    }
    /// 0 butt, 1 round, 2 square.
    pub fn cap(&mut self, k: u8) {
        let _ = writeln!(self.buf, "{k} J");
    }
    /// Mirror everything drawn after this left to right across the page.
    pub fn mirror(&mut self) {
        let _ = writeln!(self.buf, "-1 0 0 1 {PAGE_W} 0 cm");
    }
    pub fn dash(&mut self, pattern: &[f64]) {
        self.buf.push('[');
        for p in pattern {
            let _ = write!(self.buf, "{p:.2} ");
        }
        self.buf.push_str("] 0 d\n");
    }
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.nums(&[x, y], "m");
    }
    pub fn line_to(&mut self, x: f64, y: f64) {
        self.nums(&[x, y], "l");
    }
    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.move_to(x1, y1);
        self.line_to(x2, y2);
    }
    pub fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.nums(&[x1, y1, x2, y2, x3, y3], "c");
    }
    pub fn close(&mut self) {
        self.buf.push_str("h\n");
    }
    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.nums(&[x, y, w, h], "re");
    }
    pub fn circle(&mut self, cx: f64, cy: f64, r: f64) {
        let k = 0.552_284_75 * r;
        self.move_to(cx + r, cy);
        self.curve_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
        self.curve_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
        self.curve_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
        self.curve_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
        self.close();
    }
    pub fn fill(&mut self) {
        self.buf.push_str("f\n");
    }
    pub fn stroke(&mut self) {
        self.buf.push_str("S\n");
    }
    pub fn clip(&mut self) {
        self.buf.push_str("W n\n");
    }
    pub fn save(&mut self) {
        self.buf.push_str("q\n");
    }
    pub fn restore(&mut self) {
        self.buf.push_str("Q\n");
    }
    /// Text with its baseline starting at (x, y).
    pub fn text(&mut self, x: f64, y: f64, size: f64, s: &str) {
        let esc = s.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
        let _ = writeln!(self.buf, "BT /F1 1 Tf {size} 0 0 {} {x:.2} {y:.2} Tm ({esc}) Tj ET", -size);
    }
}

/// Width of a string in Helvetica, in points.
pub fn text_width(s: &str, size: f64) -> f64 {
    let units: u32 = s
        .chars()
        .map(|ch| match ch {
            ' ' | '.' | ',' | ':' | '!' => 278,
            '(' | ')' | '-' => 333,
            '?' => 556,
            'I' => 278,
            'J' => 500,
            'M' => 833,
            'W' => 944,
            'C' | 'D' | 'H' | 'N' | 'R' | 'U' => 722,
            'G' | 'O' | 'Q' => 778,
            'F' | 'T' | 'Z' => 611,
            'L' => 556,
            '0'..='9' | 'a' | 'b' | 'd' | 'e' | 'g' | 'h' | 'n' | 'o' | 'p' | 'q' | 'u' => 556,
            'c' | 'k' | 's' | 'v' | 'x' | 'y' | 'z' => 500,
            'f' | 't' => 278,
            'i' | 'j' | 'l' => 222,
            'm' => 833,
            'r' => 333,
            'w' => 722,
            _ => 667,
        })
        .sum();
    units as f64 / 1000.0 * size
}

pub struct Pdf {
    pages: Vec<Vec<u8>>,
}

impl Pdf {
    pub fn new() -> Pdf {
        Pdf { pages: Vec::new() }
    }

    pub fn add_page(&mut self, page: Canvas) {
        self.pages.push(miniz_oxide::deflate::compress_to_vec_zlib(page.buf.as_bytes(), 6));
    }

    pub fn write(&self, out: &mut impl Write) -> io::Result<()> {
        let mut doc: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let n_objs = 3 + 2 * self.pages.len();
        let mut offsets = vec![0usize; n_objs + 1];
        let mut obj = |doc: &mut Vec<u8>, id: usize, body: &[u8]| {
            offsets[id] = doc.len();
            doc.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
            doc.extend_from_slice(body);
            doc.extend_from_slice(b"\nendobj\n");
        };
        obj(&mut doc, 1, b"<< /Type /Catalog /Pages 2 0 R >>");
        let kids: Vec<String> = (0..self.pages.len()).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
        let pages = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), self.pages.len());
        obj(&mut doc, 2, pages.as_bytes());
        obj(&mut doc, 3, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>");
        for (i, content) in self.pages.iter().enumerate() {
            let page = format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] \
                 /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                5 + 2 * i
            );
            obj(&mut doc, 4 + 2 * i, page.as_bytes());
            let mut body = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", content.len()).into_bytes();
            body.extend_from_slice(content);
            body.extend_from_slice(b"\nendstream");
            obj(&mut doc, 5 + 2 * i, &body);
        }
        let xref = doc.len();
        doc.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", n_objs + 1).as_bytes());
        for off in &offsets[1..] {
            doc.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        doc.extend_from_slice(
            format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", n_objs + 1).as_bytes(),
        );
        out.write_all(&doc)
    }
}
