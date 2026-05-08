use std::collections::HashMap;
use std::ops::Range;

use nalgebra::{Matrix3, Vector3};

use crate::CreateRange;

fn transform_point(m: &Matrix3<f64>, x: f64, y: f64) -> (f64, f64) {
    let v = m * Vector3::new(x, y, 1.0);
    (v[0], v[1])
}

fn quad_corners(
    matrix: &Matrix3<f64>,
    col: u64,
    row: u64,
    w: u64,
) -> [(f64, f64); 4] {
    let left = col as f64 - 0.5;
    let right = col as f64 + w as f64 - 0.5;
    let top = row as f64 - 0.5;
    let bottom = row as f64 + 0.5;
    [
        transform_point(matrix, left, top),
        transform_point(matrix, right, top),
        transform_point(matrix, right, bottom),
        transform_point(matrix, left, bottom),
    ]
}

fn row_edges(corners: &[(f64, f64); 4], y: f64) -> Option<(f64, f64)> {
    let mut xs = [0.0f64; 4];
    let mut n = 0;
    for i in 0..4 {
        let j = (i + 1) % 4;
        let (x0, y0) = corners[i];
        let (x1, y1) = corners[j];
        if y0 == y1 {
            continue;
        }
        let (ylo, yhi) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
        if y < ylo || y > yhi {
            continue;
        }
        let t = (y - y0) / (y1 - y0);
        xs[n] = x0 + t * (x1 - x0);
        n += 1;
    }
    if n < 2 {
        return None;
    }
    xs[..n].sort_by(|a, b| a.partial_cmp(b).unwrap());
    Some((xs[0], xs[n - 1]))
}

fn rasterize_offsets(corners: &[(f64, f64); 4]) -> Vec<(i16, i16)> {
    let min_y = corners.iter().map(|c| c.1).fold(f64::MAX, f64::min);
    let max_y = corners.iter().map(|c| c.1).fold(f64::MIN, f64::max);

    let py_start = min_y.ceil() as i32;
    let py_end = max_y.floor() as i32;

    if py_start > py_end {
        return Vec::new();
    }

    let mut pixels = Vec::new();
    for py in py_start..=py_end {
        let Some((left, right)) = row_edges(corners, py as f64) else {
            continue;
        };
        let px_s = left.ceil() as i32;
        let px_e = right.floor() as i32;
        if px_s > px_e {
            continue;
        }
        for px in px_s..=px_e {
            pixels.push((px, py));
        }
    }

    if pixels.is_empty() {
        return Vec::new();
    }

    let (px0, py0) = pixels[0];
    pixels
        .into_iter()
        .map(|(px, py)| ((px - px0) as i16, (py - py0) as i16))
        .collect()
}

fn rasterize_count(corners: &[(f64, f64); 4]) -> u32 {
    let min_y = corners.iter().map(|c| c.1).fold(f64::MAX, f64::min);
    let max_y = corners.iter().map(|c| c.1).fold(f64::MIN, f64::max);

    let py_start = min_y.ceil() as i32;
    let py_end = max_y.floor() as i32;

    if py_start > py_end {
        return 0;
    }

    let mut count = 0u32;
    for py in py_start..=py_end {
        let Some((left, right)) = row_edges(corners, py as f64) else {
            continue;
        };
        let px_s = left.ceil() as i32;
        let px_e = right.floor() as i32;
        if px_s <= px_e {
            count += (px_e - px_s + 1) as u32;
        }
    }
    count
}

fn first_pixel(corners: &[(f64, f64); 4]) -> Option<(i32, i32)> {
    let min_y = corners.iter().map(|c| c.1).fold(f64::MAX, f64::min);
    let max_y = corners.iter().map(|c| c.1).fold(f64::MIN, f64::max);

    let py_start = min_y.ceil() as i32;
    let py_end = max_y.floor() as i32;

    for py in py_start..=py_end {
        if let Some((left, right)) = row_edges(corners, py as f64) {
            let px = left.ceil() as i32;
            if px <= right.floor() as i32 {
                return Some((px, py));
            }
        }
    }
    None
}

struct HeapEntry {
    current: u32,
    base_x: i32,
    base_y: i32,
    offset_idx: u32,
    left: u32,
}

pub struct AffineTransformHeap {
    heap: Vec<HeapEntry>,
    offsets: Vec<(i16, i16)>,
    width: u32,
    height: u32,
    last_popped: Option<u32>,
    pending: Option<u32>,
}

impl AffineTransformHeap {
    pub fn new<R, I>(ranges: I, matrix: &Matrix3<f64>, width: u32, height: u32) -> Self
    where
        R: CreateRange,
        R::Item: Into<u64>,
        I: Iterator<Item = R>,
    {
        let img_w = width as u64;

        let mut segments: Vec<(u64, u64, u64)> = Vec::new();
        let mut max_width = 0u64;

        for range in ranges {
            let start: u64 = range.start().into();
            let end: u64 = range.end().into();

            let mut pos = start;
            while pos < end {
                let row = pos / img_w;
                let col_start = pos - row * img_w;
                let next_row = (row + 1) * img_w;
                let col_end_excl = end.min(next_row) - row * img_w;
                let seg_width = col_end_excl - col_start;

                max_width = max_width.max(seg_width);
                segments.push((col_start, row, seg_width));
                pos = next_row;
            }
        }

        let canonical_max = quad_corners(matrix, 0, 0, max_width);
        let offsets = rasterize_offsets(&canonical_max);

        let mut width_counts: HashMap<u64, u32> = HashMap::new();
        let mut entries: Vec<HeapEntry> = Vec::new();

        for (col_start, row, seg_width) in segments {
            let corners = quad_corners(matrix, col_start, row, seg_width);

            let Some((base_x, base_y)) = first_pixel(&corners) else {
                continue;
            };

            let count = *width_counts.entry(seg_width).or_insert_with(|| {
                let c = quad_corners(matrix, 0, 0, seg_width);
                rasterize_count(&c)
            });

            if count == 0 {
                continue;
            }

            let mut offset_idx = 0u32;
            let mut current = None;

            while current.is_none() && (offset_idx as usize) < offsets.len() {
                let (dx, dy) = offsets[offset_idx as usize];
                let px = base_x + dx as i32;
                let py = base_y + dy as i32;
                offset_idx += 1;
                if px >= 0 && (px as u32) < width && py >= 0 && (py as u32) < height {
                    current = Some(py as u32 * width + px as u32);
                }
            }

            if let Some(current) = current {
                entries.push(HeapEntry {
                    current,
                    base_x,
                    base_y,
                    offset_idx,
                    left: count - 1,
                });
            }
        }

        let mut heap = Self {
            heap: entries,
            offsets,
            width,
            height,
            last_popped: None,
            pending: None,
        };
        heap.build_heap();
        heap
    }

    fn build_heap(&mut self) {
        let n = self.heap.len();
        if n <= 1 {
            return;
        }
        for i in (0..n / 2).rev() {
            self.sift_down(i);
        }
    }

    fn sift_down(&mut self, mut i: usize) {
        let n = self.heap.len();
        loop {
            let left = 2 * i + 1;
            let right = 2 * i + 2;
            let mut smallest = i;
            if left < n && self.heap[left].current < self.heap[smallest].current {
                smallest = left;
            }
            if right < n && self.heap[right].current < self.heap[smallest].current {
                smallest = right;
            }
            if smallest == i {
                break;
            }
            self.heap.swap(i, smallest);
            i = smallest;
        }
    }

    fn advance_entry(
        entry: &mut HeapEntry,
        offsets: &[(i16, i16)],
        width: u32,
        height: u32,
    ) -> bool {
        if entry.left == 0 {
            return false;
        }
        while (entry.offset_idx as usize) < offsets.len() {
            let (dx, dy) = offsets[entry.offset_idx as usize];
            let px = entry.base_x + dx as i32;
            let py = entry.base_y + dy as i32;
            entry.offset_idx += 1;
            if px >= 0 && (px as u32) < width && py >= 0 && (py as u32) < height {
                entry.current = py as u32 * width + px as u32;
                entry.left -= 1;
                return true;
            }
        }
        false
    }

    fn pop_pixel(&mut self) -> Option<u32> {
        loop {
            let result = self.heap.first()?.current;

            let advanced = {
                let entry = &mut self.heap[0];
                let offsets = &self.offsets;
                Self::advance_entry(entry, offsets, self.width, self.height)
            };

            if advanced {
                self.sift_down(0);
            } else {
                let last = self.heap.pop()?;
                if !self.heap.is_empty() {
                    self.heap[0] = last;
                    self.sift_down(0);
                }
            }

            if self.last_popped == Some(result) {
                continue;
            }
            self.last_popped = Some(result);
            return Some(result);
        }
    }
}

impl Iterator for AffineTransformHeap {
    type Item = Range<u32>;

    fn next(&mut self) -> Option<Range<u32>> {
        let start = self.pending.take().or_else(|| self.pop_pixel())?;
        let mut end = start + 1;

        loop {
            match self.pop_pixel() {
                Some(p) if p == end => end = p + 1,
                Some(p) => {
                    self.pending = Some(p);
                    break;
                }
                None => break,
            }
        }

        Some(start..end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZero;

    use crate::{ImaskSet, Rect};

    const W7: NonZero<u32> = NonZero::new(7).unwrap();

    fn print_bitmap(w: u32, h: u32, ranges: &[Range<u32>], label: &str) {
        let mut bitmap = vec![false; (w * h) as usize];
        for idx in ranges.iter().flat_map(|r| r.clone()) {
            if (idx as usize) < bitmap.len() {
                bitmap[idx as usize] = true;
            }
        }
        eprintln!("\n{}:", label);
        for y in 0..h {
            let row: String = (0..w)
                .map(|x| if bitmap[(y * w + x) as usize] { '#' } else { '.' })
                .collect();
            eprintln!("  {}", row);
        }
    }

    #[test]
    fn rotate_l_90deg_cw_about_center() {
        let l_ranges: Vec<std::ops::Range<u32>> = vec![
            0..1,
            7..8,
            14..15,
            21..22,
            28..34,
        ];

        print_bitmap(7, 7, &l_ranges, "Input L-shape");

        let ranges = l_ranges.into_iter().with_bounds(W7, W7);

        let cx = 3.0_f64;
        let cy = 3.0_f64;
        let matrix = Matrix3::new(
            0.0, 1.0, cx - cy,
            -1.0, 0.0, cx + cy,
            0.0, 0.0, 1.0,
        );

        let heap = AffineTransformHeap::new(ranges, &matrix, 7, 7);
        let result: Vec<Range<u32>> = heap.collect();

        let expected: Vec<Range<u32>> = vec![11..12, 18..19, 25..26, 32..33, 39..40, 42..47];
        assert_eq!(result, expected);

        print_bitmap(7, 7, &result, "Output after 90° CW rotation");
    }

    #[test]
    fn translate_shape_out_of_image() {
        let rect = Rect::new(2u32, 2, NonZero::new(3).unwrap(), NonZero::new(3).unwrap());
        let ranges = rect.into_rect_iter::<std::ops::Range<u32>>(W7);

        let tx = 100.0_f64;
        let ty = 200.0_f64;
        let matrix = Matrix3::new(
            1.0, 0.0, tx,
            0.0, 1.0, ty,
            0.0, 0.0, 1.0,
        );

        let heap = AffineTransformHeap::new(ranges, &matrix, 7, 7);
        let result: Vec<Range<u32>> = heap.collect();
        assert!(result.is_empty());
    }

    #[test]
    fn scale_from_center_no_gaps() {
        let rect = Rect::new(2u32, 2, NonZero::new(3).unwrap(), NonZero::new(3).unwrap());
        let ranges = rect.into_rect_iter::<std::ops::Range<u32>>(W7);

        let cx = 3.0_f64;
        let cy = 3.0_f64;
        let scale = 2.0_f64;
        let matrix = Matrix3::new(
            scale, 0.0, cx * (1.0 - scale),
            0.0, scale, cy * (1.0 - scale),
            0.0, 0.0, 1.0,
        );

        let heap = AffineTransformHeap::new(ranges, &matrix, 7, 7);
        let result: Vec<Range<u32>> = heap.collect();

        let expected: Vec<Range<u32>> = vec![0..49];
        assert_eq!(result, expected);
    }

    #[test]
    fn rotate_20x20_square_30deg_sorted_disjoint() {
        let w50: NonZero<u32> = NonZero::new(50).unwrap();
        let rect = Rect::new(15u32, 15, NonZero::new(20).unwrap(), NonZero::new(20).unwrap());
        let ranges = rect.into_rect_iter::<std::ops::Range<u32>>(w50);

        let cx = 24.5_f64;
        let cy = 24.5_f64;
        let angle = 30.0_f64.to_radians();
        let cos = angle.cos();
        let sin = angle.sin();

        let matrix = Matrix3::new(
            cos, -sin, cx * (1.0 - cos) + cy * sin,
            sin,  cos, cy * (1.0 - cos) - cx * sin,
            0.0,  0.0, 1.0,
        );

        let w = 50u32;
        let h = 50u32;
        let heap = AffineTransformHeap::new(ranges, &matrix, w, h);
        let result: Vec<Range<u32>> = heap.collect();

        for window in result.windows(2) {
            assert!(
                window[0].end <= window[1].start,
                "overlapping or out-of-order ranges: {:?}",
                window
            );
        }

        let pixel_count: u32 = result.iter().map(|r| r.end - r.start).sum();
        assert!(pixel_count > 300, "too few pixels: {}", pixel_count);
        assert!(pixel_count < 600, "too many pixels: {}", pixel_count);

        let mut img = image::GrayImage::new(w, h);
        for range in &result {
            for idx in range.clone() {
                let px = idx % w;
                let py = idx / w;
                img.put_pixel(px, py, image::Luma([255u8]));
            }
        }

        let output_dir = std::env::var("CARGO_TARGET_DIR")
            .unwrap_or_else(|_| "target".into());
        let path = format!("{}/rotate_30deg.png", output_dir);
        img.save(&path).unwrap();

        print_bitmap(w, h, &result, "20×20 square rotated 30°");
    }
}
