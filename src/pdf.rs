use crate::drawing::{Colour, Mark, screen};
use miniz_oxide::deflate::compress_to_vec_zlib;
use std::fmt::Write;

pub const LETTER: (f64, f64) = (612.0, 792.0);

const COMPRESSION: u8 = 9;
const CURVE: f64 = 0.552_284_75;

pub fn pdf(pages: &[Vec<Mark>]) -> Vec<u8> {
    let (width, height) = LETTER;
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            (0..pages.len())
                .map(|i| format!("{} 0 R", 3 + 2 * i))
                .collect::<Vec<_>>()
                .join(" "),
            pages.len()
        )
        .into_bytes(),
    ];
    for (i, marks) in pages.iter().enumerate() {
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Contents {} 0 R >>",
                4 + 2 * i
            )
            .into_bytes(),
        );
        let content = compress_to_vec_zlib(content(marks, height).as_bytes(), COMPRESSION);
        let mut stream = format!(
            "<< /Length {} /Filter /FlateDecode >>\nstream\n",
            content.len()
        )
        .into_bytes();
        stream.extend(content);
        stream.extend(b"\nendstream");
        objects.push(stream);
    }
    let mut file = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(file.len());
        file.extend(format!("{} 0 obj\n", i + 1).into_bytes());
        file.extend(object);
        file.extend(b"\nendobj\n");
    }
    let xref = file.len();
    let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        writeln!(table, "{offset:010} 00000 n ").unwrap();
    }
    write!(
        table,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    )
    .unwrap();
    file.extend(table.into_bytes());
    file
}

fn content(marks: &[Mark], height: f64) -> String {
    let mut out = String::new();
    let up = |(x, y): (f64, f64)| (x, height - y);
    for mark in &screen(marks) {
        match mark {
            Mark::Lines {
                lines,
                width,
                colour,
                dash,
                round,
            } => {
                let (cap, join) = if *round { (1, 1) } else { (0, 0) };
                let pattern = match dash {
                    Some([on, off]) => format!("[{on:.3} {off:.3}] 0 d"),
                    None => "[] 0 d".to_string(),
                };
                writeln!(
                    out,
                    "{} {width:.3} w {cap} J {join} j {pattern}",
                    paint(*colour, true)
                )
                .unwrap();
                for line in lines {
                    for (i, &point) in line.iter().enumerate() {
                        let (x, y) = up(point);
                        let verb = if i == 0 { "m" } else { "l" };
                        write!(out, "{x:.2} {y:.2} {verb} ").unwrap();
                    }
                }
                out.push_str("S\n");
            }
            Mark::Disc {
                centre,
                radius,
                colour,
            } => {
                let (x, y) = up(*centre);
                write!(out, "{} {:.2} {y:.2} m ", paint(*colour, false), x + radius).unwrap();
                let around = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0), (1.0, 0.0)];
                for pair in around.windows(2) {
                    let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
                    let pull = radius * CURVE;
                    write!(
                        out,
                        "{:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c ",
                        x + radius * ax + pull * bx,
                        y + radius * ay + pull * by,
                        x + radius * bx + pull * ax,
                        y + radius * by + pull * ay,
                        x + radius * bx,
                        y + radius * by,
                    )
                    .unwrap();
                }
                out.push_str("f\n");
            }
            Mark::Polygon { points, colour } => {
                write!(out, "{} ", paint(*colour, false)).unwrap();
                for (i, &point) in points.iter().enumerate() {
                    let (x, y) = up(point);
                    let verb = if i == 0 { "m" } else { "l" };
                    write!(out, "{x:.2} {y:.2} {verb} ").unwrap();
                }
                out.push_str("h f\n");
            }
            Mark::Shade { .. } => unreachable!("shades are screened into dots"),
        }
    }
    out
}

fn paint(colour: Colour, stroke: bool) -> String {
    let Colour { red, green, blue } = colour;
    if red == green && green == blue {
        let verb = if stroke { "K" } else { "k" };
        format!("0 0 0 {:.3} {verb}", 1.0 - red)
    } else {
        let verb = if stroke { "RG" } else { "rg" };
        format!("{red:.3} {green:.3} {blue:.3} {verb}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_reference_points_at_each_object() {
        let page = vec![Mark::Disc {
            centre: (100.0, 100.0),
            radius: 5.0,
            colour: Colour::BLACK,
        }];
        let file = pdf(&[page.clone(), page]);
        assert!(file.starts_with(b"%PDF-1.4\n"));
        let xref = file.windows(6).rposition(|w| w == b"\nxref\n").unwrap() + 1;
        let tail = String::from_utf8(file[xref..].to_vec()).unwrap();
        let listed: Vec<usize> = tail
            .lines()
            .skip(3)
            .take_while(|line| line.ends_with(" n "))
            .map(|line| line[..10].parse().unwrap())
            .collect();
        assert_eq!(listed.len(), 6);
        for (i, offset) in listed.into_iter().enumerate() {
            assert!(file[offset..].starts_with(format!("{} 0 obj", i + 1).as_bytes()));
        }
        let startxref: usize = tail.lines().rev().nth(1).unwrap().parse().unwrap();
        assert_eq!(startxref, xref);
    }
}
