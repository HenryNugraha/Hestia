/// The shorter side of a censored picture's copy, in pixels.  Stretched over a
/// card, a copy this small looks like a strong blur: colors and rough shapes,
/// nothing more.
const CENSOR_COPY_SHORT_SIDE: usize = 32;

/// How many points of the picture each copy pixel averages, along each side.
/// A fixed count keeps a copy just as cheap for a large picture as a small one.
const CENSOR_COPY_SAMPLES: usize = 4;

/// Censored copies are covered with this dark color at this opacity, out of
/// 255.  The blur already hides the picture, so this only keeps a censored card
/// darker than the rest.
const CENSOR_COVER: [u32; 3] = [12, 12, 14];
const CENSOR_COVER_ALPHA: u32 = 150;

/// A tiny, blurred and darkened copy of a picture, drawn in its place while
/// it is censored.  The main window and the overlay both use it.
pub(crate) fn censor_copy(image: &egui::ColorImage) -> egui::ColorImage {
    let [width, height] = image.size;
    let shorter = width.min(height);
    if shorter == 0 {
        return image.clone();
    }
    let (copy_width, copy_height) = if shorter <= CENSOR_COPY_SHORT_SIDE {
        (width, height)
    } else {
        let scaled = |side: usize| ((side * CENSOR_COPY_SHORT_SIDE + shorter / 2) / shorter).max(1);
        (scaled(width), scaled(height))
    };

    // Pictures load while Hestia draws, so this stays plain loops over points
    // worked out once: debug builds don't optimize closures and iterators.
    let columns = censor_copy_sample_points(width, copy_width);
    let rows = censor_copy_sample_points(height, copy_height);
    let count = (CENSOR_COPY_SAMPLES * CENSOR_COPY_SAMPLES) as u32;
    let mut pixels = Vec::with_capacity(copy_width * copy_height);
    for row_points in rows.as_chunks::<CENSOR_COPY_SAMPLES>().0 {
        for column_points in columns.as_chunks::<CENSOR_COPY_SAMPLES>().0 {
            let mut sum = [0_u32; 4];
            for &y in row_points {
                let row = &image.pixels[y * width..(y + 1) * width];
                for &x in column_points {
                    let [red, green, blue, alpha] = row[x].to_array();
                    sum[0] += u32::from(red);
                    sum[1] += u32::from(green);
                    sum[2] += u32::from(blue);
                    sum[3] += u32::from(alpha);
                }
            }
            for total in &mut sum {
                *total = (*total + count / 2) / count;
            }
            pixels.push(sum);
        }
    }

    // Soften the edges between copy pixels, twice in each direction.
    for _ in 0..2 {
        censor_copy_blur(&mut pixels, copy_width, copy_height, true);
        censor_copy_blur(&mut pixels, copy_width, copy_height, false);
    }

    let pixels = pixels
        .into_iter()
        .map(|[red, green, blue, alpha]| {
            // The colors carry their alpha already, so the cover does too.
            let darken = |channel: u32, cover: u32| {
                (channel * (255 - CENSOR_COVER_ALPHA) / 255
                    + cover * CENSOR_COVER_ALPHA * alpha / (255 * 255)) as u8
            };
            egui::Color32::from_rgba_premultiplied(
                darken(red, CENSOR_COVER[0]),
                darken(green, CENSOR_COVER[1]),
                darken(blue, CENSOR_COVER[2]),
                alpha as u8,
            )
        })
        .collect();
    egui::ColorImage::new([copy_width, copy_height], pixels)
}

/// The points along one side of a picture that the copy averages:
/// `CENSOR_COPY_SAMPLES` in a row for each copy pixel, spread evenly over the
/// part of the picture that pixel stands for.
fn censor_copy_sample_points(side: usize, copy_side: usize) -> Vec<usize> {
    let mut points = Vec::with_capacity(copy_side * CENSOR_COPY_SAMPLES);
    for copy_index in 0..copy_side {
        let start = copy_index * side / copy_side;
        let span = ((copy_index + 1) * side / copy_side - start).max(1);
        for sample in 0..CENSOR_COPY_SAMPLES {
            let point = start + (2 * sample + 1) * span / (2 * CENSOR_COPY_SAMPLES);
            points.push(point.min(side - 1));
        }
    }
    points
}

/// A 1-2-1 blur along each row, or down each column; the edges repeat their
/// last pixel.
fn censor_copy_blur(pixels: &mut [[u32; 4]], width: usize, height: usize, along_rows: bool) {
    let source = pixels.to_vec();
    for y in 0..height {
        for x in 0..width {
            let (before, after) = if along_rows {
                (
                    y * width + x.saturating_sub(1),
                    y * width + (x + 1).min(width - 1),
                )
            } else {
                (
                    y.saturating_sub(1) * width + x,
                    (y + 1).min(height - 1) * width + x,
                )
            };
            let here = y * width + x;
            for channel in 0..4 {
                pixels[here][channel] = (source[before][channel]
                    + 2 * source[here][channel]
                    + source[after][channel]
                    + 2)
                    / 4;
            }
        }
    }
}

#[cfg(test)]
mod censor_copy_tests {
    use super::*;

    fn plain(width: usize, height: usize, color: egui::Color32) -> egui::ColorImage {
        egui::ColorImage::new([width, height], vec![color; width * height])
    }

    #[test]
    fn copies_are_tiny_whatever_the_picture_size() {
        let wide = censor_copy(&plain(640, 360, egui::Color32::WHITE));
        assert_eq!(wide.size, [57, 32]);
        let tall = censor_copy(&plain(1080, 1920, egui::Color32::WHITE));
        assert_eq!(tall.size, [32, 57]);
        // Small pictures keep their size; there is nothing to shrink.
        let small = censor_copy(&plain(20, 10, egui::Color32::WHITE));
        assert_eq!(small.size, [20, 10]);
        let empty = censor_copy(&plain(0, 0, egui::Color32::WHITE));
        assert_eq!(empty.size, [0, 0]);
    }

    #[test]
    fn censored_pictures_darken_without_turning_black() {
        let copy = censor_copy(&plain(64, 64, egui::Color32::from_rgb(255, 0, 128)));
        // A plain picture only darkens, edges included.
        assert!(
            copy.pixels
                .iter()
                .all(|pixel| pixel.to_array() == [112, 7, 60, 255])
        );
    }

    #[test]
    fn censored_pictures_lose_their_details() {
        // A fine black and white checkerboard blurs into an even grey.
        let pixels = (0..64 * 64)
            .map(|index| {
                let (x, y) = (index % 64, index / 64);
                if (x + y) % 2 == 0 {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::BLACK
                }
            })
            .collect();
        let copy = censor_copy(&egui::ColorImage::new([64, 64], pixels));
        let reds = copy.pixels.iter().map(|pixel| pixel.r());
        let (darkest, lightest) = (reds.clone().min().unwrap(), reds.max().unwrap());
        assert!(lightest - darkest <= 2, "{darkest}..={lightest}");
    }

    #[test]
    fn see_through_parts_stay_see_through() {
        let copy = censor_copy(&plain(64, 64, egui::Color32::TRANSPARENT));
        assert!(
            copy.pixels
                .iter()
                .all(|pixel| *pixel == egui::Color32::TRANSPARENT)
        );
    }
}
