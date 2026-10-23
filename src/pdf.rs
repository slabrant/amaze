use crate::drawing::{Colour, Mark};
use std::fmt::Write;

pub const LETTER: (f64, f64) = (612.0, 792.0);

const CURVE: f64 = 0.552_284_75;

pub fn pdf(pages: &[Vec<Mark>]) -> Vec<u8> {
    let (width, height) = LETTER;
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            (0..pages.len())
                .map(|i| format!("{} 0 R", 3 + 2 * i))
                .collect::<Vec<_>>()
                .join(" "),
            pages.len()
        ),
    ];
    for (i, marks) in pages.iter().enumerate() {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Contents {} 0 R >>",
            4 + 2 * i
        ));
        let content = content(marks, height);
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ));
    }
    let mut file = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(file.len());
        write!(file, "{} 0 obj\n{object}\nendobj\n", i + 1).unwrap();
    }
    let xref = file.len();
    write!(file, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).unwrap();
    for offset in offsets {
        writeln!(file, "{offset:010} 00000 n ").unwrap();
    }
    write!(
        file,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    )
    .unwrap();
    file.into_bytes()
}

fn content(marks: &[Mark], height: f64) -> String {
    let mut out = String::new();
    let up = |(x, y): (f64, f64)| (x, height - y);
    for mark in marks {
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
                    "{} RG {width:.3} w {cap} J {join} j {pattern}",
                    rgb(*colour)
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
                write!(out, "{} rg {:.2} {y:.2} m ", rgb(*colour), x + radius).unwrap();
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
                write!(out, "{} rg ", rgb(*colour)).unwrap();
                for (i, &point) in points.iter().enumerate() {
                    let (x, y) = up(point);
                    let verb = if i == 0 { "m" } else { "l" };
                    write!(out, "{x:.2} {y:.2} {verb} ").unwrap();
                }
                out.push_str("h f\n");
            }
        }
    }
    out
}

fn rgb(colour: Colour) -> String {
    format!("{:.3} {:.3} {:.3}", colour.red, colour.green, colour.blue)
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
        let file = String::from_utf8(pdf(&[page.clone(), page])).unwrap();
        assert!(file.starts_with("%PDF-1.4\n"));
        let xref = file.find("\nxref\n").unwrap() + 1;
        let listed: Vec<usize> = file[xref..]
            .lines()
            .skip(3)
            .take_while(|line| line.ends_with(" n "))
            .map(|line| line[..10].parse().unwrap())
            .collect();
        assert_eq!(listed.len(), 6);
        for (i, offset) in listed.into_iter().enumerate() {
            assert!(file[offset..].starts_with(&format!("{} 0 obj", i + 1)));
        }
        let startxref: usize = file.lines().rev().nth(1).unwrap().parse().unwrap();
        assert_eq!(startxref, xref);
    }
}
