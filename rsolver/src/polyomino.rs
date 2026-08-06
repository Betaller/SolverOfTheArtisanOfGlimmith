//! Polyomino shape generation, rotation, and reflection.

use crate::types::Shape;

/// All 8 symmetries of a free polyomino (rotation × reflection).
pub fn transforms(shape: &Shape) -> Vec<Vec<[isize; 2]>> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    let signed: Vec<[isize; 2]> = shape
        .iter()
        .map(|&[r, c]| [r as isize, c as isize])
        .collect();

    for &rotate in &[0, 1, 2, 3] {
        for &reflect in &[false, true] {
            let mut t: Vec<[isize; 2]> = signed
                .iter()
                .map(|&[r, c]| {
                    let (mut rr, mut cc) = (r, c);
                    if reflect {
                        cc = -cc;
                    }
                    for _ in 0..rotate {
                        (rr, cc) = (-cc, rr);
                    }
                    [rr, cc]
                })
                .collect();
            // Normalize
            let min_r = t.iter().map(|xy| xy[0]).min().unwrap_or(0);
            let min_c = t.iter().map(|xy| xy[1]).min().unwrap_or(0);
            for xy in &mut t {
                xy[0] -= min_r;
                xy[1] -= min_c;
            }
            t.sort();
            if seen.insert(t.clone()) {
                result.push(t);
            }
        }
    }
    result
}
