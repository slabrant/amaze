use crate::drawing::{Colour, Mark};
use std::fmt::Write;

pub const A4: (f64, f64) = (595.28, 841.89);

const CURVE: f64 = 0.552_284_75;

pub fn pdf(pages: &[Vec<Mark>]) -> Vec<u8> {
    let (width, height) = A4;
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
    let mut out = String::from("1 j 0 J\n");
    let up = |(x, y): (f64, f64)| (x, height - y);
    for mark in marks {
        match mark {
            Mark::Lines {
                lines,
                width,
                colour,
            } => {
                writeln!(out, "{} RG {width:.3} w", rgb(*colour)).unwrap();
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
                let r = *radius;
                let k = r * CURVE;
                writeln!(
                    out,
                    "{} rg {:.2} {y:.2} m {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} c {:.2} {:.2} {:.2} {:.2} {x:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {y:.2} c f",
                    rgb(*colour),
                    x + r,
                    x + r,
                    y + k,
                    x + k,
                    y + r,
                    y + r,
                    x - k,
                    y + r,
                    x - r,
                    y + k,
                    x - r,
                    x - r,
                    y - k,
                    x - k,
                    y - r,
                    y - r,
                    x + k,
                    y - r,
                    x + r,
                    y - k,
                    x + r,
                )
                .unwrap();
            }
            Mark::Square {
                centre,
                half,
                colour,
            } => {
                let (x, y) = up(*centre);
                writeln!(
                    out,
                    "{} rg {:.2} {:.2} {:.2} {:.2} re f",
                    rgb(*colour),
                    x - half,
                    y - half,
                    2.0 * half,
                    2.0 * half
                )
                .unwrap();
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
        let page = vec![Mark::Square {
            centre: (100.0, 100.0),
            half: 5.0,
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
